use super::*;

impl TuiApp {
    pub(super) fn apply_mux_event(&mut self, event: DaemonEvent) {
        match event {
            DaemonEvent::TerminalSubscriptionClosed {
                session_id,
                subscription_id,
                generation,
                reason,
            } => self.handle_terminal_subscription_closed(
                session_id,
                subscription_id,
                generation,
                reason,
            ),
            DaemonEvent::PackageEvent {
                subscription_id,
                owner,
                name,
                payload,
            } => self.handle_package_event(subscription_id, owner, name, payload),
            DaemonEvent::EventGap {
                subscription_id,
                owner,
                name,
            } => self.handle_event_gap(subscription_id, owner, name),
            other => {
                let _ = other;
            }
        }
    }

    pub(super) fn desired_notice_subscriptions(&self) -> Vec<NoticeSubscriptionEntry> {
        let Some(subject) = self.selected_session.clone() else {
            return Vec::new();
        };
        let mut desired = BTreeMap::<NoticeSubscriptionKey, NoticeSubscriptionEntry>::new();
        for package in &self.packages {
            for descriptor in &package.notice_reactions {
                let key = (descriptor.owner.clone(), descriptor.name.clone());
                desired
                    .entry(key)
                    .or_insert_with(|| NoticeSubscriptionEntry {
                        descriptor: descriptor.clone(),
                        subject: subject.clone(),
                        state: EventSubscriptionState::Idle,
                    });
            }
        }
        desired.into_values().collect()
    }

    pub(super) fn sync_notice_subscriptions(&mut self) {
        let mut desired = self.desired_notice_subscriptions();
        desired.sort_by(|left, right| {
            (
                left.descriptor.owner.as_str(),
                left.descriptor.name.as_str(),
            )
                .cmp(&(
                    right.descriptor.owner.as_str(),
                    right.descriptor.name.as_str(),
                ))
        });
        let dropped = desired.len().saturating_sub(MAX_NOTICE_SUBSCRIPTIONS);
        if dropped > 0 {
            desired.truncate(MAX_NOTICE_SUBSCRIPTIONS);
            self.notice_overflow_dropped = dropped;
            self.error = Some(format!(
                "notice subscriptions dropped {dropped} descriptors over the {MAX_NOTICE_SUBSCRIPTIONS} connection limit"
            ));
        } else {
            self.notice_overflow_dropped = 0;
        }
        let desired_keys: BTreeSet<NoticeSubscriptionKey> = desired
            .iter()
            .map(|entry| {
                (
                    entry.descriptor.owner.clone(),
                    entry.descriptor.name.clone(),
                )
            })
            .collect();

        let stale: Vec<NoticeSubscriptionKey> = self
            .notice_subscriptions
            .keys()
            .filter(|key| !desired_keys.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            self.unsubscribe_notice_entry(&key);
        }

        for entry in desired {
            let key = (
                entry.descriptor.owner.clone(),
                entry.descriptor.name.clone(),
            );
            match self.notice_subscriptions.get(&key) {
                Some(current)
                    if current.subject != entry.subject
                        || matches!(current.state, EventSubscriptionState::Idle) =>
                {
                    if matches!(current.state, EventSubscriptionState::Idle) {
                        self.notice_subscriptions.remove(&key);
                    } else {
                        self.unsubscribe_notice_entry(&key);
                    }
                    self.subscribe_notice_entry(entry);
                }
                Some(_) => {
                    if let Some(current) = self.notice_subscriptions.get_mut(&key) {
                        current.descriptor = entry.descriptor;
                    }
                }
                None => self.subscribe_notice_entry(entry),
            }
        }
    }

    pub(super) fn subscribe_notice_entry(&mut self, mut entry: NoticeSubscriptionEntry) {
        let subscription_id = format!(
            "btui-events-{}-{}",
            short_suffix(),
            entry.descriptor.name.replace('.', "-")
        );
        let key = (
            entry.descriptor.owner.clone(),
            entry.descriptor.name.clone(),
        );
        entry.state = EventSubscriptionState::Candidate(subscription_id.clone());
        self.notice_subscription_by_id
            .insert(subscription_id.clone(), key.clone());
        self.notice_subscriptions.insert(key.clone(), entry.clone());
        self.submit(
            DaemonRequest::SubscribeEvents {
                subscription_id: subscription_id.clone(),
                owner: entry.descriptor.owner.clone(),
                name: entry.descriptor.name.clone(),
                subjects: vec![entry.subject.clone()],
            },
            PendingReply::SubscribeEvents {
                key,
                subscription_id,
            },
            REQUEST_DEADLINE,
        );
    }

    /// EventSubscribed promotes the candidate and replays events parked while
    /// the response was outstanding.
    pub(super) fn complete_notice_subscription(
        &mut self,
        key: &NoticeSubscriptionKey,
        subscription_id: &str,
        response: DaemonResponse,
    ) {
        self.record_diagnostics(response.diagnostics);
        if response.kind == DaemonResponseKind::EventSubscribed && response.error.is_none() {
            let promoted = match self.notice_subscriptions.get_mut(key) {
                Some(current) if current.state.candidate_id() == Some(subscription_id) => {
                    current.state = EventSubscriptionState::Active(subscription_id.to_string());
                    true
                }
                _ => false,
            };
            if promoted {
                self.promote_parked_notice_events(subscription_id);
            } else {
                self.notice_parked.remove(subscription_id);
            }
            return;
        }
        let detail = response
            .error
            .as_ref()
            .map(|error| error.message.clone())
            .unwrap_or_else(|| format!("{:?}", response.kind));
        if let Some(error) = response.error {
            self.record_diagnostics(error.diagnostics);
        }
        self.reject_event_subscription_candidate(
            subscription_id,
            format!("event subscription was not accepted: {detail}"),
        );
    }

    pub(super) fn unsubscribe_notice_entry(&mut self, key: &NoticeSubscriptionKey) {
        let Some(entry) = self.notice_subscriptions.remove(key) else {
            return;
        };
        let subscription_id = match &entry.state {
            EventSubscriptionState::Idle => None,
            EventSubscriptionState::Candidate(id) | EventSubscriptionState::Active(id) => {
                Some(id.clone())
            }
        };
        if let Some(subscription_id) = subscription_id {
            self.notice_subscription_by_id.remove(&subscription_id);
            self.notice_parked.remove(&subscription_id);
            if !matches!(entry.state, EventSubscriptionState::Idle) && self.is_connected() {
                self.submit(
                    DaemonRequest::UnsubscribeEvents { subscription_id },
                    PendingReply::Unsubscribe,
                    REQUEST_DEADLINE,
                );
            }
        }
    }

    pub(super) fn reject_event_subscription_candidate(
        &mut self,
        subscription_id: &str,
        message: String,
    ) {
        if let Some(key) = self.notice_subscription_by_id.get(subscription_id).cloned()
            && let Some(entry) = self.notice_subscriptions.get_mut(&key)
            && entry.state.candidate_id() == Some(subscription_id)
        {
            entry.state = EventSubscriptionState::Idle;
            self.notice_subscription_by_id.remove(subscription_id);
        }
        self.notice_parked.remove(subscription_id);
        self.error = Some(message);
    }

    pub(super) fn clear_event_subscription_state(&mut self) {
        self.notice_subscriptions.clear();
        self.notice_subscription_by_id.clear();
        self.notice_parked.clear();
        self.notice_overflow_dropped = 0;
        self.transient_notice = None;
    }

    pub(super) fn candidate_notice_entry(
        &self,
        subscription_id: &str,
    ) -> Option<&NoticeSubscriptionEntry> {
        let key = self.notice_subscription_by_id.get(subscription_id)?;
        let entry = self.notice_subscriptions.get(key)?;
        (entry.state.candidate_id() == Some(subscription_id)).then_some(entry)
    }

    pub(super) fn handle_package_event(
        &mut self,
        subscription_id: String,
        owner: String,
        name: String,
        payload: Value,
    ) {
        if self.active_notice_entry(&subscription_id).is_some() {
            self.apply_active_package_event(&subscription_id, &owner, &name, &payload);
            return;
        }
        // Hub may complete SubscribeEvents after the first event on the new
        // subscription is already delivered. Park a bounded tail until
        // EventSubscribed promotes the candidate.
        if self.candidate_notice_entry(&subscription_id).is_some() {
            let parked = self.notice_parked.entry(subscription_id).or_default();
            if parked.events.len() >= MAX_PARKED_NOTICE_EVENTS {
                parked.events.pop_front();
                parked.gap = true;
            }
            parked.events.push_back((owner, name, payload));
        }
    }

    pub(super) fn apply_active_package_event(
        &mut self,
        subscription_id: &str,
        owner: &str,
        name: &str,
        payload: &Value,
    ) {
        let Some(entry) = self.active_notice_entry(subscription_id) else {
            return;
        };
        if entry.descriptor.owner != owner || entry.descriptor.name != name {
            return;
        }
        let text_pointer = entry.descriptor.text_pointer.clone();
        let ttl_ms = entry.descriptor.ttl_ms;
        match resolve_notice_text(payload, &text_pointer) {
            Ok(text) => {
                self.transient_notice = Some(TransientNotice {
                    text: text.to_string(),
                    // timer: ui-lifetime — transient notice, server-supplied ttl_ms; one wake at expiry via next_deadline
                    deadline: Instant::now() + Duration::from_millis(u64::from(ttl_ms)),
                });
            }
            Err(error) => {
                self.transient_notice = None;
                self.error = Some(error.to_string());
            }
        }
    }

    pub(super) fn handle_event_gap(
        &mut self,
        subscription_id: String,
        owner: String,
        name: String,
    ) {
        if self.active_notice_entry(&subscription_id).is_some() {
            self.apply_active_event_gap(&subscription_id, &owner, &name);
            return;
        }
        if self.candidate_notice_entry(&subscription_id).is_some() {
            self.notice_parked.entry(subscription_id).or_default().gap = true;
        }
    }

    pub(super) fn apply_active_event_gap(
        &mut self,
        subscription_id: &str,
        owner: &str,
        name: &str,
    ) {
        let Some(entry) = self.active_notice_entry(subscription_id) else {
            return;
        };
        if entry.descriptor.owner != owner || entry.descriptor.name != name {
            return;
        }
        self.transient_notice = None;
        self.error = Some("package event gap; durable package state is unchanged".to_string());
    }

    pub(super) fn promote_parked_notice_events(&mut self, subscription_id: &str) {
        let Some(parked) = self.notice_parked.remove(subscription_id) else {
            return;
        };
        let (owner, name) = match self.active_notice_entry(subscription_id) {
            Some(entry) => (
                entry.descriptor.owner.clone(),
                entry.descriptor.name.clone(),
            ),
            None => return,
        };
        if parked.gap {
            self.apply_active_event_gap(subscription_id, &owner, &name);
        }
        for (event_owner, event_name, payload) in parked.events {
            self.apply_active_package_event(subscription_id, &event_owner, &event_name, &payload);
        }
    }

    pub(super) fn expire_transient_notice(&mut self) {
        if self
            .transient_notice
            .as_ref()
            .is_some_and(|notice| Instant::now() >= notice.deadline)
        {
            self.transient_notice = None;
        }
    }

    pub(super) fn active_notice_entry(
        &self,
        subscription_id: &str,
    ) -> Option<&NoticeSubscriptionEntry> {
        let key = self.notice_subscription_by_id.get(subscription_id)?;
        let entry = self.notice_subscriptions.get(key)?;
        if entry.state.active_id() == Some(subscription_id) {
            Some(entry)
        } else {
            None
        }
    }

    /// Informational, not an error: the terminal re-attached on its own.
    pub(super) fn recovery_notice_line(&self) -> Option<UiNode> {
        let notice = self.recovery_notice.as_ref()?;
        Some(node(
            UiNodeKind::Text,
            "workspace-recovery-notice",
            json!({ "text": format!("reconnected {}× after {}", notice.count, notice.cause) }),
        ))
    }

    pub(super) fn transient_notice_band(&self) -> Option<UiNode> {
        let notice = self.transient_notice.as_ref()?;
        if Instant::now() >= notice.deadline {
            return None;
        }
        Some(node(
            UiNodeKind::Text,
            "workspace-transient-notice",
            json!({ "text": notice.text }),
        ))
    }
}
