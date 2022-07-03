//! GGTE01 (USA) opcodes

// This file provides constants for the various opcodes that may appear in event data.
//
// Each constant is written in both decimal and hexadecimal here, but it seems like the game
// developers used decimal because some groups of opcodes start at multiples of 100.
//
// Opcodes with unknown meanings are labeled with "Unk<num>" and their names may change to reflect
// new information.

use super::{CmdOp, ExprOp, MsgOp, TypeOp};
use crate::opcodes;

opcodes! {
    /// GGTE01 (USA) opcode map
    pub struct Ggte;

    CmdOp {
        Abort = 1, // 01
        Return = 2, // 02
        Goto = 3, // 03
        Set = 4, // 04
        If = 5, // 05
        Elif = 6, // 06
        EndIf = 7, // 07
        Case = 8, // 08
        Expr = 9, // 09
        While = 10, // 0a
        Break = 11, // 0b
        Run = 12, // 0c
        Lib = 13, // 0d
        PushBp = 14, // 0e
        PopBp = 15, // 0f
        SetSp = 16, // 10
        Anim = 17, // 11
        Anim1 = 18, // 12
        Anim2 = 19, // 13
        Attach = 20, // 14
        Born = 21, // 15
        Call = 22, // 16
        Camera = 23, // 17
        Check = 24, // 18
        Color = 25, // 19
        Detach = 26, // 1a
        Dir = 27, // 1b
        MDir = 28, // 1c
        Disp = 29, // 1d
        Kill = 30, // 1e
        Light = 31, // 1f
        Menu = 32, // 20
        Move = 33, // 21
        MoveTo = 34, // 22
        Msg = 35, // 23
        Pos = 36, // 24
        PrintF = 37, // 25
        Ptcl = 38, // 26
        Read = 39, // 27
        Scale = 40, // 28
        MScale = 41, // 29
        Scrn = 42, // 2a
        Select = 43, // 2b
        Sfx = 44, // 2c
        Timer = 45, // 2d
        Wait = 46, // 2e
        Warp = 47, // 2f
        Win = 48, // 30
        Movie = 49, // 31
    }

    ExprOp {
        Equal = 0, // 00
        NotEqual = 1, // 01
        Less = 2, // 02
        LessEqual = 3, // 03
        Greater = 4, // 04
        GreaterEqual = 5, // 05
        Not = 6, // 06
        Add = 7, // 07
        Subtract = 8, // 08
        Multiply = 9, // 09
        Divide = 10, // 0a
        Modulo = 11, // 0b
        BitAnd = 12, // 0c
        BitOr = 13, // 0d
        BitXor = 14, // 0e
        AddAssign = 15, // 0f
        SubtractAssign = 16, // 10
        MultiplyAssign = 17, // 11
        DivideAssign = 18, // 12
        ModuloAssign = 19, // 13
        BitAndAssign = 20, // 14
        BitOrAssign = 21, // 15
        BitXorAssign = 22, // 16
        Imm16 = 23, // 17
        Imm32 = 24, // 18
        AddressOf = 25, // 19
        Stack = 26, // 1a
        ParentStack = 27, // 1b
        Flag = 28, // 1c
        Variable = 29, // 1d
        Result1 = 30, // 1e
        Result2 = 31, // 1f
        Pad = 32, // 20
        Battery = 100, // 64
        Money = 101, // 65
        Item = 102, // 66
        Atc = 103, // 67
        Rank = 104, // 68
        Exp = 105, // 69
        Level = 106, // 6a
        Hold = 107, // 6b
        Map = 108, // 6c
        ActorName = 109, // 6d
        ItemName = 110, // 6e
        Time = 111, // 6f
        CurrentSuit = 112, // 70
        Scrap = 113, // 71
        CurrentAtc = 114, // 72
        Use = 115, // 73
        Hit = 116, // 74
        StickerName = 117, // 75
        Obj = 200, // c8
        Random = 201, // c9
        Sin = 202, // ca
        Cos = 203, // cb
        ArrayElement = 204, // cc
    }

    TypeOp {
        Time = 200, // c8
        Unk201 = 201, // c9
        Wipe = 202, // ca
        Unk203 = 203, // cb
        Anim = 204, // cc
        Dir = 205, // cd
        Move = 206, // ce
        Pos = 207, // cf
        Obj = 208, // d0
        Unk209 = 209, // d1
        Unk210 = 210, // d2
        Unk211 = 211, // d3
        PosX = 212, // d4
        PosY = 213, // d5
        PosZ = 214, // d6
        BoneX = 215, // d7
        BoneY = 216, // d8
        BoneZ = 217, // d9
        DirTo = 218, // da
        Color = 219, // db
        Lead = 220, // dc
        Sfx = 221, // dd
        Modulate = 222, // de
        Blend = 223, // df
        Real = 224, // e0
        Cam = 225, // e1
        Unk226 = 226, // e2
        Unk227 = 227, // e3
        Distance = 228, // e4
        Unk229 = 229, // e5
        Unk230 = 230, // e6
        Unk231 = 231, // e7
        Unk232 = 232, // e8
        Read = 233, // e9
        Unk234 = 234, // ea
        Unk235 = 235, // eb
        Unk236 = 236, // ec
        Unk237 = 237, // ed
        Unk238 = 238, // ee
        Unk239 = 239, // ef
        Unk240 = 240, // f0
        Unk241 = 241, // f1
        Unk242 = 242, // f2
        Unk243 = 243, // f3
        Scale = 244, // f4
        Cue = 245, // f5
        Unk246 = 246, // f6
        Unk247 = 247, // f7
        Unk248 = 248, // f8
        Unk249 = 249, // f9
        Unk250 = 250, // fa
        Unk251 = 251, // fb
        Unk252 = 252, // fc
    }

    MsgOp {
        End = 0, // 00
        Speed = 1, // 01
        Wait = 2, // 02
        Anim = 3, // 03
        Sfx = 4, // 04
        Voice = 5, // 05
        Default = 6, // 06
        Newline = 10, // 0a
        NewlineVt = 11, // 0b
        Format = 12, // 0c
        Size = 13, // 0d
        Color = 14, // 0e
        Rgba = 15, // 0f
        Proportional = 16, // 10
        Icon = 17, // 11
        Shake = 18, // 12
        Center = 19, // 13
        Rotate = 20, // 14
        Scale = 21, // 15
        NumInput = 22, // 16
        Question = 23, // 17
        Stay = 24, // 18
    }
}
