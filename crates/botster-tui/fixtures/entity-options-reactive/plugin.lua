-- Owner package for botster-tui reactive entity-options live proof.
-- Source family is process-wide /session so live Hub ordered upsert/patch/remove
-- frames update options on the active subscription without surface re-render.
-- Dual-family exclude matrix is covered by the shared Hub fixture unit path.

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
            source = "/session",
            value_field = "session_uuid",
            display_fields = { "lifecycle_class", "session_type_id", "registry_state" },
            order = { "session_uuid" },
            where = { lifecycle_class = "current" },
          },
        },
      },
    },
  }
end

local function handle_action(request)
  local action_id = request.action_id
  local values = request.values or {}

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
      id = "submit_action",
      kind = "ui_action",
      descriptor_id = "entity-options.submit",
      descriptor = {
        action_id = "entity-options.submit",
        surface_id = "entity-options-reactive.picker",
      },
      call = handle_action,
    },
  },
})
