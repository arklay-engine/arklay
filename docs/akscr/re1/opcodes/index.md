# RE 1 opcodes

Every RE 1 instruction, by purpose, with a page each. The command VM runs a
room's init and main scripts; the event instructions are only valid inside
an event (see [Event scripts](../index.md#event-scripts)), and each event
state accepts its own set. The opcode is in hex; the command and event
opcode spaces are separate, so the same number names different
instructions in each.

On each page, the parameters are the arguments as a script writes them,
one per field the engine reads. Padding or unused fields are written as 0.

## Commands written by field
The `.s` disassembler prints some commands byte by byte, or with words split
into bytes. In `.akscr` these take one argument per field the engine reads,
at its width:

`obj`, `item_aot_set`, `enemy`, `effect`, `effect_tracked`,
`effect_kill_b`, `timer_setup`, `voice_play`, `setw`, `inst_cfg`,
`msg_list`, `eml_pos`, `eml_rot`, `cam_set`, `plw_anim`, `model_op`,
`objtbl_b_set`, `ck_anim`, `ck_bits`, `ck_counter`, `ck_tween`,
`ck_item_count`, `ck_last_item`, `aot_switch`, `msgnode_set`,
`snd_pan_vol_set`, `bgm_volume_ramp`, `task_kill`, `spd_set`, `spd_add`,
`swap_var`, `mass_mask`, `costume_set`, `costume_ck`, and the event
instructions `act_motion`, `act_motion_path`, `act_anim_seq`,
`act_anim_flags`, `act_anim_set`, `act_flag_op`, `act_param_set`,
`tw_set_pos`, `tw_set_rot`, `tw_abs_pos`, `tw_set_field`, `tw_set_sel` and
`tw_abs_rot`.

The compiler packs the fields back into the bytes, so `asm { }` blocks keep
the `.s` shapes. The tables above give the `.akscr` form.

{{#opcode-index Flow | Conditions | Flags and state | Room actions (AOTs) | Events | Camera, sprites and light | Messages, voice and film | Sound and music | Enemies and characters | The player | Objects and models | Effects | Event: top state | Event: control | Event: actor state | Event: tween state}}
