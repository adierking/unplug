//! GGTE01 (USA) opcodes

// This file provides constants for the various opcodes that may appear in event data.
//
// Each constant is written in both decimal and hexadecimal here, but it seems like the game
// developers used decimal because some groups of opcodes start at multiples of 100.
//
// Opcodes with unknown meanings are labeled with "UNK_<num>" and their names may change to reflect
// new information.

// Commands
pub const CMD_ABORT: u8 = 1; // 01
pub const CMD_RETURN: u8 = 2; // 02
pub const CMD_GOTO: u8 = 3; // 03
pub const CMD_SET: u8 = 4; // 04
pub const CMD_IF: u8 = 5; // 05
pub const CMD_ELIF: u8 = 6; // 06
pub const CMD_ENDIF: u8 = 7; // 07
pub const CMD_CASE: u8 = 8; // 08
pub const CMD_EXPR: u8 = 9; // 09
pub const CMD_WHILE: u8 = 10; // 0a
pub const CMD_BREAK: u8 = 11; // 0b
pub const CMD_RUN: u8 = 12; // 0c
pub const CMD_LIB: u8 = 13; // 0d
pub const CMD_PUSHBP: u8 = 14; // 0e
pub const CMD_POPBP: u8 = 15; // 0f
pub const CMD_SETSP: u8 = 16; // 10
pub const CMD_ANIM: u8 = 17; // 11
pub const CMD_ANIM1: u8 = 18; // 12
pub const CMD_ANIM2: u8 = 19; // 13
pub const CMD_ATTACH: u8 = 20; // 14
pub const CMD_BORN: u8 = 21; // 15
pub const CMD_CALL: u8 = 22; // 16
pub const CMD_CAMERA: u8 = 23; // 17
pub const CMD_CHECK: u8 = 24; // 18
pub const CMD_COLOR: u8 = 25; // 19
pub const CMD_DETACH: u8 = 26; // 1a
pub const CMD_DIR: u8 = 27; // 1b
pub const CMD_MDIR: u8 = 28; // 1c
pub const CMD_DISP: u8 = 29; // 1d
pub const CMD_KILL: u8 = 30; // 1e
pub const CMD_LIGHT: u8 = 31; // 1f
pub const CMD_MENU: u8 = 32; // 20
pub const CMD_MOVE: u8 = 33; // 21
pub const CMD_MOVETO: u8 = 34; // 22
pub const CMD_MSG: u8 = 35; // 23
pub const CMD_POS: u8 = 36; // 24
pub const CMD_PRINTF: u8 = 37; // 25
pub const CMD_PTCL: u8 = 38; // 26
pub const CMD_READ: u8 = 39; // 27
pub const CMD_SCALE: u8 = 40; // 28
pub const CMD_MSCALE: u8 = 41; // 29
pub const CMD_SCRN: u8 = 42; // 2a
pub const CMD_SELECT: u8 = 43; // 2b
pub const CMD_SFX: u8 = 44; // 2c
pub const CMD_TIMER: u8 = 45; // 2d
pub const CMD_WAIT: u8 = 46; // 2e
pub const CMD_WARP: u8 = 47; // 2f
pub const CMD_WIN: u8 = 48; // 30
pub const CMD_MOVIE: u8 = 49; // 31

// Expression opcodes
pub const OP_EQUAL: u8 = 0; // 00
pub const OP_NOT_EQUAL: u8 = 1; // 01
pub const OP_LESS: u8 = 2; // 02
pub const OP_LESS_EQUAL: u8 = 3; // 03
pub const OP_GREATER: u8 = 4; // 04
pub const OP_GREATER_EQUAL: u8 = 5; // 05
pub const OP_NOT: u8 = 6; // 06
pub const OP_ADD: u8 = 7; // 07
pub const OP_SUBTRACT: u8 = 8; // 08
pub const OP_MULTIPLY: u8 = 9; // 09
pub const OP_DIVIDE: u8 = 10; // 0a
pub const OP_MODULO: u8 = 11; // 0b
pub const OP_BIT_AND: u8 = 12; // 0c
pub const OP_BIT_OR: u8 = 13; // 0d
pub const OP_BIT_XOR: u8 = 14; // 0e
pub const OP_ADD_ASSIGN: u8 = 15; // 0f
pub const OP_SUBTRACT_ASSIGN: u8 = 16; // 10
pub const OP_MULTIPLY_ASSIGN: u8 = 17; // 11
pub const OP_DIVIDE_ASSIGN: u8 = 18; // 12
pub const OP_MODULO_ASSIGN: u8 = 19; // 13
pub const OP_BIT_AND_ASSIGN: u8 = 20; // 14
pub const OP_BIT_OR_ASSIGN: u8 = 21; // 15
pub const OP_BIT_XOR_ASSIGN: u8 = 22; // 16
pub const OP_CONST_16: u8 = 23; // 17
pub const OP_CONST_32: u8 = 24; // 18
pub const OP_ADDRESS_OF: u8 = 25; // 19
pub const OP_STACK: u8 = 26; // 1a
pub const OP_PARENT_STACK: u8 = 27; // 1b
pub const OP_FLAG: u8 = 28; // 1c
pub const OP_VARIABLE: u8 = 29; // 1d
pub const OP_RESULT_1: u8 = 30; // 1e
pub const OP_RESULT_2: u8 = 31; // 1f
pub const OP_PAD: u8 = 32; // 20
pub const OP_BATTERY: u8 = 100; // 64
pub const OP_MONEY: u8 = 101; // 65
pub const OP_ITEM: u8 = 102; // 66
pub const OP_ATC: u8 = 103; // 67
pub const OP_RANK: u8 = 104; // 68
pub const OP_EXP: u8 = 105; // 69
pub const OP_LEVEL: u8 = 106; // 6a
pub const OP_HOLD: u8 = 107; // 6b
pub const OP_MAP: u8 = 108; // 6c
pub const OP_ACTOR_NAME: u8 = 109; // 6d
pub const OP_ITEM_NAME: u8 = 110; // 6e
pub const OP_TIME: u8 = 111; // 6f
pub const OP_CURRENT_SUIT: u8 = 112; // 70
pub const OP_SCRAP: u8 = 113; // 71
pub const OP_CURRENT_ATC: u8 = 114; // 72
pub const OP_USE: u8 = 115; // 73
pub const OP_HIT: u8 = 116; // 74
pub const OP_STICKER_NAME: u8 = 117; // 75
pub const OP_OBJ: u8 = 200; // c8
pub const OP_RANDOM: u8 = 201; // c9
pub const OP_SIN: u8 = 202; // ca
pub const OP_COS: u8 = 203; // cb
pub const OP_ARRAY_ELEMENT: u8 = 204; // cc

// Command types
pub const TYPE_TIME: i32 = 200; // c8
pub const TYPE_UNK_201: i32 = 201; // c9
pub const TYPE_WIPE: i32 = 202; // ca
pub const TYPE_UNK_203: i32 = 203; // cb
pub const TYPE_ANIM: i32 = 204; // cc
pub const TYPE_DIR: i32 = 205; // cd
pub const TYPE_MOVE: i32 = 206; // ce
pub const TYPE_POS: i32 = 207; // cf
pub const TYPE_OBJ: i32 = 208; // d0
pub const TYPE_UNK_209: i32 = 209; // d1
pub const TYPE_UNK_210: i32 = 210; // d2
pub const TYPE_UNK_211: i32 = 211; // d3
pub const TYPE_POS_X: i32 = 212; // d4
pub const TYPE_POS_Y: i32 = 213; // d5
pub const TYPE_POS_Z: i32 = 214; // d6
pub const TYPE_BONE_X: i32 = 215; // d7
pub const TYPE_BONE_Y: i32 = 216; // d8
pub const TYPE_BONE_Z: i32 = 217; // d9
pub const TYPE_DIR_TO: i32 = 218; // da
pub const TYPE_COLOR: i32 = 219; // db
pub const TYPE_LEAD: i32 = 220; // dc
pub const TYPE_SFX: i32 = 221; // dd
pub const TYPE_MODULATE: i32 = 222; // de
pub const TYPE_BLEND: i32 = 223; // df
pub const TYPE_REAL: i32 = 224; // e0
pub const TYPE_CAM: i32 = 225; // e1
pub const TYPE_UNK_226: i32 = 226; // e2
pub const TYPE_UNK_227: i32 = 227; // e3
pub const TYPE_DISTANCE: i32 = 228; // e4
pub const TYPE_UNK_229: i32 = 229; // e5
pub const TYPE_UNK_230: i32 = 230; // e6
pub const TYPE_UNK_231: i32 = 231; // e7
pub const TYPE_UNK_232: i32 = 232; // e8
pub const TYPE_READ: i32 = 233; // e9
pub const TYPE_UNK_234: i32 = 234; // ea
pub const TYPE_UNK_235: i32 = 235; // eb
pub const TYPE_UNK_236: i32 = 236; // ec
pub const TYPE_UNK_237: i32 = 237; // ed
pub const TYPE_UNK_238: i32 = 238; // ee
pub const TYPE_UNK_239: i32 = 239; // ef
pub const TYPE_UNK_240: i32 = 240; // f0
pub const TYPE_UNK_241: i32 = 241; // f1
pub const TYPE_UNK_242: i32 = 242; // f2
pub const TYPE_UNK_243: i32 = 243; // f3
pub const TYPE_SCALE: i32 = 244; // f4
pub const TYPE_CUE: i32 = 245; // f5
pub const TYPE_UNK_246: i32 = 246; // f6
pub const TYPE_UNK_247: i32 = 247; // f7
pub const TYPE_UNK_248: i32 = 248; // f8
pub const TYPE_UNK_249: i32 = 249; // f9
pub const TYPE_UNK_250: i32 = 250; // fa
pub const TYPE_UNK_251: i32 = 251; // fb
pub const TYPE_UNK_252: i32 = 252; // fc

// Message commands
pub const MSG_END: u8 = 0; // 00
pub const MSG_SPEED: u8 = 1; // 01
pub const MSG_WAIT: u8 = 2; // 02
pub const MSG_ANIM: u8 = 3; // 03
pub const MSG_SFX: u8 = 4; // 04
pub const MSG_VOICE: u8 = 5; // 05
pub const MSG_DEFAULT: u8 = 6; // 06
pub const MSG_NEWLINE: u8 = 10; // 0a
pub const MSG_NEWLINE_VT: u8 = 11; // 0b
pub const MSG_FORMAT: u8 = 12; // 0c
pub const MSG_SIZE: u8 = 13; // 0d
pub const MSG_COLOR: u8 = 14; // 0e
pub const MSG_RGBA: u8 = 15; // 0f
pub const MSG_PROPORTIONAL: u8 = 16; // 10
pub const MSG_ICON: u8 = 17; // 11
pub const MSG_SHAKE: u8 = 18; // 12
pub const MSG_CENTER: u8 = 19; // 13
pub const MSG_ROTATE: u8 = 20; // 14
pub const MSG_SCALE: u8 = 21; // 15
pub const MSG_NUM_INPUT: u8 = 22; // 16
pub const MSG_QUESTION: u8 = 23; // 17
pub const MSG_STAY: u8 = 24; // 18
pub const MSG_OPCODE_MAX: u8 = 24;

// Message wait types
pub const MSG_WAIT_SUIT_MENU: u8 = 252; // fc
pub const MSG_WAIT_ATC_MENU: u8 = 253; // fd
pub const MSG_WAIT_LEFT_PLUG: u8 = 254; // fe
pub const MSG_WAIT_RIGHT_PLUG: u8 = 255; // ff

// SFX commands
pub const SFX_WAIT: i32 = -1; // ff (only supported in messages)
pub const SFX_STOP: i32 = 0; // 00
pub const SFX_PLAY: i32 = 1; // 01
pub const SFX_FADE_OUT: i32 = 2; // 02
pub const SFX_FADE_IN: i32 = 3; // 03
pub const SFX_FADE: i32 = 4; // 04
pub const SFX_UNK_5: i32 = 5; // 05
pub const SFX_UNK_6: i32 = 6; // 06
pub const SFX_UNK_245: i32 = 245; // f5 (not supported in messages. TYPE_CUE?)
