-- Owner package for botster-tui reactive entity-options live proof.
-- Source family only (exclude matrix is covered by the shared Hub fixture
-- unit path). Surface actions mutate in-memory provider state; a client
-- resubscribe delivers a fresh authoritative snapshot without surface re-render.

local items = {
  {
    id = "opt-alpha",
    label = "Alpha",
    lifecycle_class = "current",
    session_type = "agent",
    spawn_point = "local",
    value = "opt-alpha",
  },
  {
    id = "opt-bravo",
    label = "Bravo",
    lifecycle_class = "current",
    session_type = "agent",
    spawn_point = "local",
    value = "opt-bravo",
  },
  {
    id = "opt-charlie",
    label = "Charlie",
    lifecycle_class = "current",
    session_type = "agent",
    spawn_point = "remote",
    value = "opt-charlie",
  },
}

local item_generation = 0

local function copy_items()
  local out = {}
  for _, item in ipairs(items) do
    out[#out + 1] = {
      id = item.id,
      label = item.label,
      lifecycle_class = item.lifecycle_class,
      session_type = item.session_type,
      spawn_point = item.spawn_point,
      value = item.value,
    }
  end
  return out
end

local function picker_surface(_arguments)
  return {
    type = "form",
    id = "entity-options-form",
    props = {
      action = { id = "entity-options.submit" },
      submit_label = "Submit selection",
    },
    children = {
      {
        type = "select",
        id = "entity-options-select",
        props = {
          name = "option",
          label = "Option",
          options_source = {
            ["$kind"] = "entity_options",
            source = "/entity-options-reactive.item",
            value_field = "value",
            display_fields = { "label", "lifecycle_class", "session_type", "spawn_point" },
            order = { "label", "value" },
            where = { lifecycle_class = "current" },
          },
        },
      },
      {
        type = "button",
        id = "entity-options-remove-alpha",
        props = {
          label = "Remove Alpha",
          action = { id = "entity-options.remove", payload = { value = "opt-alpha" } },
        },
      },
    },
  }
end

local function handle_action(request)
  local action_id = request.action_id
  local values = request.values or {}
  local payload = request.payload or {}

  if action_id == "entity-options.remove" then
    local target = payload.value
    local next_items = {}
    for _, item in ipairs(items) do
      if item.value ~= target then
        next_items[#next_items + 1] = item
      end
    end
    items = next_items
    item_generation = item_generation + 1
    return {
      request_id = request.request_id,
      surface_id = request.surface_id,
      action_id = action_id,
      node_id = request.node_id,
      state = "accepted",
      payload = { removed = target, generation = item_generation },
    }
  end

  if action_id == "entity-options.submit" then
    local selected = values.option
    if type(selected) ~= "string" or selected == "" then
      return {
        request_id = request.request_id,
        surface_id = request.surface_id,
        action_id = action_id,
        node_id = request.node_id,
        state = "rejected",
        form_errors = { "option is required" },
      }
    end
    return {
      request_id = request.request_id,
      surface_id = request.surface_id,
      action_id = action_id,
      node_id = request.node_id,
      state = "accepted",
      payload = { selected = selected },
      normalized_values = { option = selected },
    }
  end

  return {
    request_id = request.request_id,
    surface_id = request.surface_id,
    action_id = action_id,
    node_id = request.node_id,
    state = "rejected",
    error = "unknown action",
  }
end

return botster.register({
  handlers = {
    {
      id = "picker",
      kind = "surface_route",
      descriptor_id = "entity-options-reactive.picker",
      call = picker_surface,
    },
    {
      id = "items",
      kind = "entity_provider",
      descriptor_id = "entity-options-reactive.item",
      descriptor = { entity_type = "entity-options-reactive.item", id_field = "id" },
      call = function(request)
        item_generation = item_generation + 1
        return {
          type = "entity_snapshot",
          entity_type = "entity-options-reactive.item",
          snapshot_seq = item_generation,
          items = copy_items(),
          subscription_id = request.subscription_id,
        }
      end,
    },
    {
      id = "submit_action",
      kind = "ui_action",
      descriptor_id = "entity-options.submit",
      descriptor = {
        action_id = "entity-options.submit",
        surface_id = "entity-options-reactive.picker",
      },
      call = handle_action,
    },
    {
      id = "remove_action",
      kind = "ui_action",
      descriptor_id = "entity-options.remove",
      descriptor = {
        action_id = "entity-options.remove",
        surface_id = "entity-options-reactive.picker",
      },
      call = handle_action,
    },
  },
})
