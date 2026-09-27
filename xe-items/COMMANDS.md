# XeItems commands and permissions

The command is `/xeitems`, with the alias `/xi`. Arguments are tab-completed in game.

## Permissions

- Every subcommand needs two nodes: `XeItems:command` plus its own `XeItems:<subcommand>`
  (for example `XeItems:give`).
- `/xi help` and plain `/xi` (which shows the help) only need `XeItems:command`.
- `XeItems:admin` grants every XeItems node.
- By default, all nodes go to operators of level 2 or higher. Change this with `op_level`
  in `plugins/data/XeItems/config.toml`.

## Creating and editing items

| Command | Permission | What it does |
|---|---|---|
| `/xi create <id> <material>` | `XeItems:create` | Creates a new item |
| `/xi edit <id>` | `XeItems:edit` | Opens the visual editor |
| `/xi name <id> [lang:<en_us\|es_es>] <text...>` | `XeItems:name` | Sets the name, optionally for one language |
| `/xi lore <id> add <text...> \| remove <line> \| clear` | `XeItems:lore` | Adds, removes or clears lore lines |
| `/xi enchant <id> <enchantment> <level>` | `XeItems:enchant` | Adds an enchantment (level 0 removes it) |
| `/xi amount <id> <amount>` | `XeItems:amount` | Sets how many one craft gives |
| `/xi material <id> <material>` | `XeItems:material` | Changes the base material |
| `/xi set <id> ...` | `XeItems:set` | Sets properties (see [set](#set)) |
| `/xi attribute <id> <attribute> <amount> [operation] [slot] [level:<n>] [tier:<n>]` | `XeItems:attribute` | Adds an attribute modifier (amount 0 removes it) |
| `/xi attribute <id> clear` | `XeItems:attribute` | Removes every attribute modifier |
| `/xi ability <id> <trigger>[:name] ...` | `XeItems:ability` | Adds or changes abilities (see [ability](#ability)) |
| `/xi progression <id> ...` | `XeItems:progression` | Configures tier and level progression (see [progression](#progression)) |
| `/xi drop <id> <mob\|*> <chance>` | `XeItems:drop` | Sets a mob drop (0.02 = 2 %, 0 removes it) |
| `/xi drop <id> clear` | `XeItems:drop` | Removes every drop |
| `/xi recipe <id> shaped <row1/row2/row3> <K=item,...>` | `XeItems:recipe` | Adds a shaped recipe |
| `/xi recipe <id> shapeless <item,item,...>` | `XeItems:recipe` | Adds a shapeless recipe |
| `/xi recipe <id> cooking <item> [smelting\|blasting\|smoking\|campfire] [ticks]` | `XeItems:recipe` | Adds a furnace-type recipe |
| `/xi recipe <id> remove <recipe>` | `XeItems:recipe` | Removes a recipe |
| `/xi clone <id> <new_id>` | `XeItems:clone` | Copies an item under a new id |
| `/xi fromhand <id>` | `XeItems:fromhand` | Creates an item from the one you're holding |
| `/xi import <json>` | `XeItems:import` | Creates an item from JSON |
| `/xi delete <id>` | `XeItems:delete` | Deletes an item (its recipes stay active until the server restarts) |

## Giving, viewing and maintenance

| Command | Permission | What it does |
|---|---|---|
| `/xi help` | `XeItems:command` only | Lists the commands |
| `/xi give <id> [player] [amount] [rarity]` | `XeItems:give` | Gives copies of an item (more than 64 is allowed) |
| `/xi list` | `XeItems:list` | Lists all items |
| `/xi info <id>` | `XeItems:info` | Shows an item's details |
| `/xi export <id>` | `XeItems:export` | Prints the item as JSON, in chat and in the server log |
| `/xi progress <player> [info\|tier\|level\|addxp <n>\|rarity <r>\|reset]` | `XeItems:progress` | Views or changes the copy in that player's hand |
| `/xi progress #<creation id> ...` | `XeItems:progress` | Same, for any copy, using the id shown in its lore |
| `/xi update [id]` | `XeItems:update` | Rebuilds the copies online players have from the current definitions |
| `/xi reload` | `XeItems:reload` | Re-registers all items and recipes (doesn't re-read the item files) |

## set

`/xi set <id> <property> <value>`:

| Property | Value |
|---|---|
| `unbreakable`, `glint` | `true` or `false` |
| `durability`, `stack` | a number, or `none` |
| `rarity` | `common`, `uncommon`, `rare`, `epic` or `none` |
| `model` | `namespace:path`, or `none` |
| `modeldata` | a number, or `none` |
| `food` | `<nutrition> <saturation> [always] [seconds]`, or `none` |
| `craftmaterial` | a material, or `none`: what the item's recipes give, swapped for the item after crafting. Use one vanilla can't craft. |

## ability

Triggers: `right_click`, `sneak_right_click`, `left_click`, `hit`, `eat`. One trigger can
hold several abilities if you give them names, for example `hit` and `hit:inferno`.

Actions, added with `/xi ability <id> <trigger> <action> ...`:

| Action | Arguments |
|---|---|
| `effect` | `<effect> [level] [seconds] [self\|target] [N]` (the last number adds one effect level every N tiers) |
| `command` | `<command...>`, run as the console |
| `sound` | `<sound> [volume] [pitch]` |
| `particle` | `<particle> [count]` |
| `heal` | `<amount>` |
| `damage` | `<amount>`, extra damage to the target |
| `fire` | `<seconds>`, sets the target on fire |
| `message` | `<text...>` |
| `dash` | `<strength>`, launches the player where they're looking |
| `area` | `<radius> <damage> [fire seconds] [knockback]` |
| `shield` | `<amount>`, absorption hearts |
| `lifesteal` | `<fraction>` (0.2 = 20 % of the damage dealt) |

Settings, with `/xi ability <id> <trigger> ...`:

| Setting | Arguments |
|---|---|
| `cooldown` | `<ticks>` (20 = 1 second) |
| `consume` | `true` or `false`: uses up one item each time |
| `remove` | `<number>`: removes one action |
| `clear` | removes the ability |
| `require` | `<tier> [level]`: needed to use it |
| `name` | `<text...>`, shown in the lore |
| `scale` | `<per level\|default> [per tier\|default]` (0.05 = +5 % power; `default` uses the item's values) |

## progression

`/xi progression <id> ...`:

| Form | What it does |
|---|---|
| `on` or `off` | Turns tier and level progression on or off |
| `<setting> <number>` | Sets `xp_base`, `xp_growth`, `xp_per_tier`, `power_level`, `power_tier`, `cooldown_tier`, `max_tier` or `max_level` |
| `xp <hit\|kill\|right_click\|sneak_right_click\|left_click\|eat> <amount>` | XP earned per trigger |
| `cost <item> <amount per tier>` or `cost none` | What ascending to the next tier costs |
| `core <tier> <core item id>` or `core <tier> none` | The core item needed to enter a tier |
| `rarity <common\|uncommon\|rare\|epic\|legendary> <weight>` or `rarity default` | This item's rarity weights (`default` uses config.toml) |
