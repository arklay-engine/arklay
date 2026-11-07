---@meta arklay.gui

-- GUI DOM scripts: the Lua behind a markup screen's `script` attribute.
--
-- This file is the source of the "GUI DOM" page of the
-- documentation, and a LuaLS definition file for linting and completion in
-- an editor. `arklay mod lua-types --out DIR` writes every definition file
-- the engine ships; add DIR to your editor's `workspace.library`.
--
-- Doc comments (`---`) are the documentation. A `-- #` line starts a group
-- on the page; plain comments like this one are not published.

---The engine's modern menus (the system menu, options, the modern save and
---load screens, the debug menu, the launcher) are not
---[screen scripts](screen.md): they are retained screens written in XML
---markup (`assets/ui/*.xml`), built into a node tree that the engine lays
---out, paints and drives. See "GUI screen markup" in `docs/internals/architecture.md`
---for the elements, bindings and actions.
---
---A screen whose root names a script, `<screen script="name.lua">`, runs it
---in a fresh sandboxed Lua state each time it opens, so nothing carries over
---between opens. The sandbox is the hooks' (see
---[Sandbox and errors](hooks.md#sandbox-and-errors)). The chunk runs once,
---then its global `on_open(ui)` is called; there, and inside every handler
---it registers, `ui` is the live UI:
---
---```lua
---function on_open(ui)
---    local rows = ui:find("#rows")
---    for i = 1, 3 do
---        local row = ui:create('<item action="pick:' .. i .. '"/>')
---        row:set("text", "Row " .. i)
---        rows:append(row)
---    end
---    ui:set("picked", 0)
---    ui:on("action:pick", function(ui, n)
---        ui:set("picked", n)
---        if n == 3 then ui:emit("done", ui:get("picked")) end
---    end)
---end
---```
---
---An action that is not built in (`back`, `open:<screen>`, `reset`, the
---dialog actions) goes first to the script's `action:<name>` handler; only
---when the screen has no such handler does it reach the host as an output.
---The action's argument is the text after the colon, converted to an
---integer when it is one.
---
---Store values cross between Lua and the UI as `nil`, booleans, integers
---(a float is rounded), strings, sequence tables (lists) and string-keyed
---tables (maps), nested up to 16 deep; a function or userdata raises an
---error. Images read back as `nil`.
---
---An error in the script, in `on_open` or in a handler, is logged once with
---a `[ui]` prefix and closes the script's screen (and any opened above it).
---
---No pack can supply a GUI script today: the screens and their scripts are
---the engine's own, so this surface is for engine development.

-- # Entry point

---Called once when the screen opens, after its tree is built. Build nodes,
---publish store values and register handlers here.
---@param ui GuiUi
function on_open(ui) end

-- source: src/gui/dom/lua/methods.rs#Ui
---The live UI, passed to `on_open` and to every handler. It is valid only
---during that call. Methods that act on "this screen" act on the screen
---whose script is running, wherever it sits in the stack, and raise
---`the screen has closed` once it has.
---@class GuiUi
local GuiUi = {}

-- # The store

---The store value at `path`. A dotted path reaches inside a stored table:
---`saves.rows.0.tag` is field `tag` of entry 0 of the list stored at
---`saves.rows` (list indices in a path are 0-based).
---@param path string
---@return any value `nil` when nothing is stored there.
function GuiUi:get(path) end

---Publish `value` at `path`, inside a stored table when a stored key is a
---prefix of the path. Every binding that reads it updates.
---@param path string
---@param value any `nil`, a boolean, a number, a string or a table of them.
function GuiUi:set(path, value) end

-- # Nodes

---The first node of this screen matching `selector`.
---@param selector string `#id` for the node with that `id`, or an element name such as `list`.
---@return GuiNode? node `nil` when nothing matches.
function GuiUi:find(selector) end

---Build a detached node of this screen from markup. It shows once it is
---[appended](#GuiNode.append) somewhere. A markup error raises an error.
---@param xml string One element, which may use the library's components and the host's custom widgets.
---@return GuiNode
function GuiUi:create(xml) end

-- # Screens and actions

---Open the screen `name` on top of the stack. An unknown screen, or a stack
---already 16 deep, raises an error.
---@param name string
function GuiUi:open(name) end

---Close this screen and every screen opened above it.
function GuiUi:close() end

---Send an action to the host as an output, as a markup action that no
---handler takes would.
---@param name string
---@param arg any The action's value; see the store's value rules.
function GuiUi:emit(name, arg) end

---Register `handler` for `event`, replacing any earlier one. The events are
---`action:<name>`, raised when an action `name` (or `name:arg`) fires in
---this screen.
---@param event string
---@param handler fun(ui: GuiUi, arg: any) `arg` is the action's argument: an integer, a string, or `nil`.
function GuiUi:on(event, handler) end

---Internal: the call every [`GuiNode`](#GuiNode) method forwards to. Do not
---call it; use the node methods.
---@param op string
---@param id integer
---@param ... any
---@return any
function GuiUi:_node(op, id, ...) end

-- source: src/gui/dom/lua/methods.rs#NodeRef
---A node of the screen whose script created or found it. Every method
---raises `the node was removed` once the node has left the tree.
---@class GuiNode
local GuiNode = {}

---Append `child` as this node's last child, moving it from wherever it was.
---Appending a node that is not in this screen, or one of this node's own
---ancestors, raises an error.
---@param child GuiNode
function GuiNode:append(child) end

---Remove this node and its subtree. Removing the screen's root raises an
---error.
function GuiNode:remove() end

---Remove every child of this node.
function GuiNode:clear() end

---A property of this node as the screen evaluates it now: the attribute's
---binding resolved against the store.
---@param name string
---@return any value
function GuiNode:get(name) end

---Set an attribute, in markup syntax: a literal, a `{path}` binding or an
---`@key` string. `nil` removes the attribute.
---@param name string
---@param text string|number|boolean|nil
function GuiNode:set(name, text) end

---Move the focus to this node.
function GuiNode:focus() end
