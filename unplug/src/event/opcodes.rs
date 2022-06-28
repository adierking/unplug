pub mod ggte;

/// An expression opcode. Refer to `Expr` for documentation.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ExprOpcode {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Not,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    BitAnd,
    BitOr,
    BitXor,
    AddAssign,
    SubtractAssign,
    MultiplyAssign,
    DivideAssign,
    ModuloAssign,
    BitAndAssign,
    BitOrAssign,
    BitXorAssign,
    Imm16,
    Imm32,
    AddressOf,
    Stack,
    ParentStack,
    Flag,
    Variable,
    Result1,
    Result2,
    Pad,
    Battery,
    Money,
    Item,
    Atc,
    Rank,
    Exp,
    Level,
    Hold,
    Map,
    ActorName,
    ItemName,
    Time,
    CurrentSuit,
    Scrap,
    CurrentAtc,
    Use,
    Hit,
    StickerName,
    Obj,
    Random,
    Sin,
    Cos,
    ArrayElement,
}

/// A command opcode. Refer to `Command` for documentation.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum CommandOpcode {
    Abort,
    Return,
    Goto,
    Set,
    If,
    Elif,
    EndIf,
    Case,
    Expr,
    While,
    Break,
    Run,
    Lib,
    PushBp,
    PopBp,
    SetSp,
    Anim,
    Anim1,
    Anim2,
    Attach,
    Born,
    Call,
    Camera,
    Check,
    Color,
    Detach,
    Dir,
    MDir,
    Disp,
    Kill,
    Light,
    Menu,
    Move,
    MoveTo,
    Msg,
    Pos,
    PrintF,
    Ptcl,
    Read,
    Scale,
    MScale,
    Scrn,
    Select,
    Sfx,
    Timer,
    Wait,
    Warp,
    Win,
    Movie,
}

/// A command type opcode (note, these are actually represented as immediate expressions). Refer to
/// individual commands for documentation.
pub enum TypeOpcode {
    Time,
    Unk201,
    Wipe,
    Unk203,
    Anim,
    Dir,
    Move,
    Pos,
    Obj,
    Unk209,
    Unk210,
    Unk211,
    PosX,
    PosY,
    PosZ,
    BoneX,
    BoneY,
    BoneZ,
    DirTo,
    Color,
    Lead,
    Sfx,
    Modulate,
    Blend,
    Real,
    Cam,
    Unk226,
    Unk227,
    Distance,
    Unk229,
    Unk230,
    Unk231,
    Unk232,
    Read,
    Unk234,
    Unk235,
    Unk236,
    Unk237,
    Unk238,
    Unk239,
    Unk240,
    Unk241,
    Unk242,
    Unk243,
    Scale,
    Cue,
    Unk246,
    Unk247,
    Unk248,
    Unk249,
    Unk250,
    Unk251,
    Unk252,
}
