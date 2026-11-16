# GUI screen markup

Each modern screen (the system menu, options, the modern save and load screens, the debug menu, the launcher) is one XML document in this directory. Its root is a `<screen>`, and `components.xml` defines the shared parts.

- `standard-screen` gives every screen the same title (with breadcrumbs from the screens below it), a caption, a `header` slot, the content, a `footer` slot and the hint bar. Cancel goes back.
- `option-category`, `option-detail` and `save-row` are the other shared components.

**Elements**

- Containers: `stack` (`orientation`, and `gap` as a theme token), `split`, `list`, `tabs`/`tab`.
- Content: `text` (with a `variant`), `image`, `badge`, `separator`, `spacer`, `facts`, `hint-bar`.
- Interactive rows: `item`, `choice`, `toggle`, `slider`, `link` and `info`.
- Any other hyphenated name is a custom widget that the host registers.
- Sizes come from the theme. Only `width`/`height` overrides appear in the markup.

**Attributes**

- An attribute value is a literal, a `{path}` binding into the store (`item.*` inside a `<template>`, `focus.*` for the focused row), or `@key` for a GUI string.
- `items="{path}"` repeats a `<template>` once per list entry.
- `def="{path}"` fills an option row from a map with keys such as `label`, `choices`, `value` and `default`.

**Actions**

- `back`, `open:<screen>` and `reset` are built in.
- `confirm`, `note` and `confirm-if` ask in a dialog first.
- Any other `name:arg` goes to the screen's Lua script, or to the host as a typed output.
- A `<command key="..." label="..." action="..."/>` binds a key and shows its hint while the focus is inside its parent.

**Tabs**

- Tab keys follow the nesting depth: outer strips use TabPrev/TabNext, inner strips use SubPrev/SubNext.

**Scripting**

A screen with `script="name.lua"` gets a fresh sandboxed Lua state each time it opens. The script's `on_open(ui)` can:

- call `ui:get`/`ui:set` on the store;
- `ui:find`, `ui:create` and `node:append` nodes;
- register handlers with `ui:on("action:<name>", fn)`;
- `ui:emit` actions to the host.

The full table is in `src/gui/dom/lua.rs`.
