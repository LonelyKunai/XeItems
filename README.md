# XeItems (Rust)

Custom items and recipes for [Pumpkin](https://github.com/Pumpkin-MC/Pumpkin) servers
(Pumpkin 0.2.0, Minecraft 26.3), made in game with `/xeitems` (`/xi`) or a visual editor.
Built as a WebAssembly plugin of about 1.6 MB.

## Build

Needs Rust with the `wasm32-wasip2` target:

```powershell
rustup target add wasm32-wasip2
.\build.ps1                                     # build; output in target\wasm32-wasip2\release\xe_items.wasm
.\build.ps1 -Server D:\path\to\server\plugins   # build and copy it there as XeItems.wasm
```

Tests (the bundled item file loads and survives being written as TOML):

```powershell
cargo test --release -p xe-items --target <your host target, e.g. x86_64-pc-windows-gnu> --lib
```

## Layout

- `xe-items/`: the plugin. Default config: `config.toml`. Bundled items:
  `items/examples.json` (basic examples). Every command and its permission:
  [`xe-items/COMMANDS.md`](xe-items/COMMANDS.md).
- `xe-common/`: code shared with the other Xe plugins: translations (`src/lang`, English
  and Spanish), config.toml loading, data-folder files, command and permission helpers.
- `vendor/pumpkin-plugin-api/`: the Pumpkin plugin crate with the WIT files of Pumpkin
  0.2.0 / Minecraft 26.3. crates.io only has the 26.2 version, which this server would
  reject. When updating Pumpkin, copy the new server's WIT files into
  `vendor/pumpkin-plugin-api/wit/`.

## Features

- Item files: every `.json` (`{"items": [...]}`) and `.toml` (`[[items]]` tables) file in
  the data folder's `items/` is loaded, in name order; same fields in both formats. A
  later file can redefine an item of an earlier one (logged). Changes made in game are
  written back to the item's own file (TOML comments are lost when that happens); new
  items go to `custom.json`. An item that fails to load is logged and kept in its file as
  it is; a file that fails to load is never overwritten.
- Items carry their id in custom data (`<namespace>:id`), so they can be recognised
  later: `/xi update [id]` (and joining, see `update_on_join`) rebuilds them from the
  current definition, keeping count, used durability and enchantments added by players.
- Properties (`/xi set`): unbreakable, durability, stack size, rarity, glint, item model,
  custom model data, food (any material becomes edible). Glint is saved in the item but
  not put on stacks: Pumpkin writes `enchantment_glint_override` (like
  `attribute_modifiers`) as an empty tag, and the file of the player holding one can't be
  read back, so they lose their whole inventory. Old copies with it are rebuilt on join.
- Attributes (`/xi attribute`): attack damage, speed, max health... per equipment slot,
  optionally growing per level and tier.
- Abilities (`/xi ability`): on `right_click`, `sneak_right_click`, `left_click`, `hit` or
  `eat`, run effects, console commands, sounds, particles, heal, extra damage, fire, a
  dash, area damage, shield or lifesteal, with an optional cooldown and item use-up. Names
  of effects, sounds, particles and attributes come from the WIT files (`build.rs`
  generates the lookup tables). `/xi ability <id> <trigger> scale <per level> [per tier]`
  gives one ability its own power growth instead of the item's.
- Progression (`/xi progression`): each copy of an item has its own tier (I-XV), level and
  a rarity rolled when it's made; XP comes from using it. Cores and ascension costs can
  gate the next tier.
- Mob drops (`/xi drop`), per mob or for any mob.
- Crafting: on a patched Pumpkin, recipe results keep their components, so the real item
  comes out. On unpatched Pumpkin the plain item is swapped after the craft, only when a
  single XeItems item crafts that material and vanilla doesn't
  (`xe-items/src/vanilla_crafteo.txt`); the rest are listed in the log at start. A craft
  token (`/xi set <id> craftmaterial <material>`) works around it.
- API for other plugins (`xe-items/src/ipc.rs`): Pumpkin plugin messages to `"XeItems"`,
  a JSON object with an `op` in, `{"ok": true, ...}` or an error text out. Ops:
  - `{"op":"version"}` → `api` (1), `namespace`.
  - `{"op":"items"}` → `items`: `[{id, name, material, progression}]`.
  - `{"op":"give","player":"<uuid>","id":"xeitems:emberfang","amount":1,"rarity":null,"source":"...","announce":true}`
    → a new copy (rolled rarity and record, like a mob drop), into the inventory or kept
    until there is space. Replies `given`, `rarities`.
  - `{"op":"equip","world":"world","entities":[{"entity":123,"slot":"mainhand","item":"xeitems:bronze_sword"}]}`
    → plain stacks on those entities (mob gear); replies `equipped`, `errors`.
  - `{"op":"held","player":"<uuid>"}` → `id` (or null), `progression`, `tier`, `level`, `rarity`.
  - `{"op":"grant_xp","player":"<uuid>","amount":10}` → progression XP for the held copy;
    replies `granted`.

  A message runs inside XeItems before the sender's call returns: call it from scheduled
  tasks, not while handling an event XeItems may have caused.

## Permissions

`XeItems:command` to run `/xeitems`, plus `XeItems:<subcommand>` for each subcommand;
`XeItems:admin` grants all of them. Operators of `op_level` (config.toml, default 2) have
them by default. Full list: [`xe-items/COMMANDS.md`](xe-items/COMMANDS.md).

## Data files

The plugin reads and writes its data folder while the server runs (it requests the
`fs.write.data` permission):

- `plugins/data/XeItems/config.toml`: settings, created from the default the first time.
  Changes apply after a restart.
- `plugins/data/XeItems/items/*.json`, `items/*.toml`: item definitions. The bundled
  `examples.json` is written only when the folder doesn't exist; an `items.json` from before the folder
  is moved to `items/custom.json`. Items made with `/xeitems` or the editor are saved to
  their item file immediately.
- `plugins/data/XeItems/instances.json`: the progression record of every copy.
