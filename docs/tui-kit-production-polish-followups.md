# TUI production polish ownership

This client owns Botster product semantics. `botster-tui-kit` owns reusable
terminal widget mechanics and presentation behavior.

## `botster-tui` owns

- The Sessions, Apps, and Diagnostics information architecture.
- Product-specific responsive proportions and sidebar width limits.
- Session display names, lifecycle wording, and terminal titles.
- Which session is selected or attached.
- Action wording such as “New session” and “Detach current.”
- Which package and entrypoint actions are valid in each lifecycle state.

## `botster-tui-kit` owns

- A visible keyboard-focus treatment for buttons, fields, lists, and terminal
  panes that is distinct from selected or active state.
- Consistent selected-row styling beyond the existing `> ` marker.
- Toolbar spacing, label truncation, and overflow behavior at narrow widths.
- Reusable constrained split and fixed/flexible row primitives, so clients do
  not need an app-specific compositor for standard sidebar layouts.
- Scrollable long-form panels with keyboard and mouse position indicators.
- Consistent disabled, destructive, and confirmation action presentation.

Do not implement these kit concerns as one-off colors or glyphs in
`botster-tui`. Port them back when the corresponding shared primitive changes;
keep product labels and state binding local.
