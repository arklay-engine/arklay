# Voice

`voice/` holds the spoken lines. A room script plays a line by its id, and
the line's file is named by the stage the player is in and that id:

```text
voice/01_09.ogg     stage 1, line 0x09
voice/02_3b.ogg     stage 2, line 0x3b
```

A line is `voice/<stage>_<id>`: the stage as two decimal digits from `01`,
and the id as two lower-case hex digits. Formats, highest first: `ogg`,
`wav`. A line may be any length.

A pack may keep a folder of lines per scenario instead,
`voice/<scenario>/<stage>_<id>`, with the scenario a single digit:

```text
voice/0/01_0a.ogg   scenario 0, stage 1, line 0x0a
voice/1/01_0a.ogg   scenario 1, the same line
```

The scripts wait for a line to end before they go on, so a longer line
holds the scene longer. A line the pack does not have is silent, and the
subtitle that goes with it still times out.

Each line is usually paired with a message, which the engine shows as a
subtitle. The subtitle text is in the language files, not here (see
[Subtitles](language.md#subtitles)).
