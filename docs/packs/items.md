# Items

`item/` holds one folder per item, named by the item id in two lower-case
hex digits. Everything about the item is in its folder: what it does, what
it is called, its inventory icon and its 3D model.

```text
item/02/
├── item.xml                the definition: kind, stack, names
├── item.bmp                the 40x30 inventory icon
├── model.tmd               the 3D model the inventory shows
└── model.bmp               the model's 256x256 texture page
item/6e/                    a document
├── item.xml
├── file_bg.bmp             the book it is read in
├── file_fg_00_en-US.bmp    page 0, in English
├── file_fg_00_ja-JP.bmp    page 0, in Japanese
└── file_fg_01_en-US.bmp
```

| File | What | Formats |
|---|---|---|
| `item.xml` | the definition | XML |
| `item` | the inventory icon, 40x30 | `webp`, `png`, `bmp`, `tim` |
| `model.tmd` | the 3D model | TMD |
| `model` | the model's texture, a 256x256 indexed page with a full 256-colour palette | `bmp` |
| `file_bg` | a document's book | `webp`, `png`, `bmp`, `tim` |
| `file_fg_<page>_<language>` | a document's page (two hex digits) in one language | `webp`, `png`, `bmp`, `tim` |

Every file is optional:

- no `item.xml`: the item has no name, no stack and kind `other`;
- no icon: the item has no icon;
- no `model.tmd`: the item has no model;
- a `model.tmd` without a texture: the model is untextured.

The folder holds no id of its own, so copying a folder to another id, or
into another pack, moves the whole item.

## The definition

`item.xml` says how the item behaves and what it is called:

```xml
<item kind="heal" stack="1">
    <effects>
        <effect kind="heal" amount="third" />
        <effect kind="cure_poison" />
    </effects>
    <strings>
        <language lang="en-US">
            <string key="name">Mixed Herbs</string>
            <string key="description">It's a mixture of green\nand blue herbs.</string>
        </language>
        <language lang="fr-FR">
            <string key="name">Herbes mélangées</string>
        </language>
    </strings>
</item>
```

| Attribute or element | Meaning |
|---|---|
| `kind` | `weapon`, `ammo`, `bottle`, `chemical`, `special`, `key`, `book`, `heal`, `file` or `other` (the default). It picks what *Use* does and which message an item that cannot be used shows. |
| `stack` | the most the item holds: a weapon's loaded rounds, an ammo box's count. Leave it out for an item whose quantity is not shown. |
| `merge="true"` | picking the item up adds to a stack of it the player already carries, instead of taking a new slot |
| `<effect kind="heal" amount>` | restores health when used: `third`, `two_thirds`, `full`, or a number of points |
| `<effect kind="cure_poison">`, `<effect kind="cure_serious_poison">` | clears poisoning when used |
| `<strings>` | the item's text, one `<language lang>` per language |

The strings are `name`, `name_unchecked` (the name shown until the item is
examined) and `description`, written as in the language files (`\n` for a
new line, `{color 1}`). They become the `ITEM_<id>_NAME`,
`ITEM_<id>_NAME_UNCHECKED` and `ITEM_<id>_DESCRIPTION` keys of their
language, and a `language/<tag>.txt` that sets the same key still wins (see
[Language](language.md)).

A pack's `item.xml` replaces that item's definition whole, so a mod
restates every field it keeps. A definition that does not parse is
reported and the item is left undefined, and unknown elements and
attributes are errors: run `arklay verify` on a mod to catch a typo.

## Icons and models

The icon's black is see-through. To hide an item's icon, ship a fully
transparent 40x30 image.

`model.tmd` is the TMD mesh the inventory and the item viewer turn, and
`model.bmp` its texture (see [BMP pages](index.md#bmp-pages)).

Two sheets of several items' art sit at the top of `item/`:
`item/item_mix` and `item/medal`.

For HD art, the per-item names are under `htexture/`:

```text
htexture/item/0b/item.webp     the icon, covering its 40x30 cell
htexture/item/0b/model.webp    the model's 256x256 page
```

## Documents

The items from `5f` on are the documents the player can read: document `n`
is item `5f + n`, and its definition's kind is `file`. Its folder holds
the reader's art: `file_bg` is the book it is shown in, and
`file_fg_<page>_<language>` its pages, one per language. The reader's own
art, the two book covers of the document list and the page arrows, is
`ui/file_book_0`, `ui/file_book_1` and `ui/file_arrows`.
