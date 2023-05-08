use phf::phf_map;
use unplug::event::opcodes::{CmdOp, ExprOp, MsgOp, Opcode, TypeOp};

/// An opcode which has an associated name.
pub trait NamedOpcode: Opcode {
    /// Looks up the opcode corresponding to `name` and returns it if found.
    fn get(name: &str) -> Option<Self>;

    /// Returns the opcode's name.
    fn name(self) -> &'static str;
}

/// An opcode which appears as part of a message.
///
/// We don't use `MsgOp` directly because we want full text strings rather than single characters.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AsmMsgOp {
    Speed,
    Wait,
    Anim,
    Sfx,
    Voice,
    Default,
    Format,
    Size,
    Color,
    Rgba,
    Proportional,
    Icon,
    Shake,
    Center,
    Rotate,
    Scale,
    NumInput,
    Question,
    Stay,
    Text,
    #[default]
    Invalid,
}

impl TryFrom<AsmMsgOp> for MsgOp {
    type Error = AsmMsgOp;
    fn try_from(op: AsmMsgOp) -> Result<Self, Self::Error> {
        Ok(match op {
            AsmMsgOp::Speed => Self::Speed,
            AsmMsgOp::Wait => Self::Wait,
            AsmMsgOp::Anim => Self::Anim,
            AsmMsgOp::Sfx => Self::Sfx,
            AsmMsgOp::Voice => Self::Voice,
            AsmMsgOp::Default => Self::Default,
            AsmMsgOp::Format => Self::Format,
            AsmMsgOp::Size => Self::Size,
            AsmMsgOp::Color => Self::Color,
            AsmMsgOp::Rgba => Self::Rgba,
            AsmMsgOp::Proportional => Self::Proportional,
            AsmMsgOp::Icon => Self::Icon,
            AsmMsgOp::Shake => Self::Shake,
            AsmMsgOp::Center => Self::Center,
            AsmMsgOp::Rotate => Self::Rotate,
            AsmMsgOp::Scale => Self::Scale,
            AsmMsgOp::NumInput => Self::NumInput,
            AsmMsgOp::Question => Self::Question,
            AsmMsgOp::Stay => Self::Stay,
            AsmMsgOp::Invalid => Self::Invalid,
            AsmMsgOp::Text => return Err(op),
        })
    }
}

impl TryFrom<MsgOp> for AsmMsgOp {
    type Error = MsgOp;
    fn try_from(op: MsgOp) -> Result<Self, Self::Error> {
        Ok(match op {
            MsgOp::Speed => Self::Speed,
            MsgOp::Wait => Self::Wait,
            MsgOp::Anim => Self::Anim,
            MsgOp::Sfx => Self::Sfx,
            MsgOp::Voice => Self::Voice,
            MsgOp::Default => Self::Default,
            MsgOp::Format => Self::Format,
            MsgOp::Size => Self::Size,
            MsgOp::Color => Self::Color,
            MsgOp::Rgba => Self::Rgba,
            MsgOp::Proportional => Self::Proportional,
            MsgOp::Icon => Self::Icon,
            MsgOp::Shake => Self::Shake,
            MsgOp::Center => Self::Center,
            MsgOp::Rotate => Self::Rotate,
            MsgOp::Scale => Self::Scale,
            MsgOp::NumInput => Self::NumInput,
            MsgOp::Question => Self::Question,
            MsgOp::Stay => Self::Stay,
            MsgOp::Invalid => Self::Invalid,
            MsgOp::Char(_) | MsgOp::Newline | MsgOp::NewlineVt | MsgOp::End => return Err(op),
        })
    }
}

impl Opcode for AsmMsgOp {
    type Value = u8;
    fn map_unrecognized(value: Self::Value) -> Result<Self, Self::Value> {
        Err(value)
    }
    fn map_unsupported(opcode: Self) -> Result<Self::Value, Self> {
        Err(opcode)
    }
}

/// An assembler directive type.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DirOp {
    Globals,
    Stage,
    Byte,
    Word,
    Dword,
    Lib,
    Prologue,
    Startup,
    Dead,
    Pose,
    TimeCycle,
    TimeUp,
    Interact,
    #[default]
    Invalid,
}

impl Opcode for DirOp {
    type Value = u8;
    fn map_unrecognized(value: Self::Value) -> Result<Self, Self::Value> {
        Err(value)
    }
    fn map_unsupported(opcode: Self) -> Result<Self::Value, Self> {
        Err(opcode)
    }
}

/// Associates names with opcode enums by implementing `NamedOpcode` for them.
macro_rules! opcode_names {
    {
        $(
            $type:ty {
                $($value:ident => $name:literal),*
                $(,)*
            }
        )*
    } => {
        $(
            impl NamedOpcode for $type {
                fn get(name: &str) -> Option<Self> {
                    static VALUES: phf::Map<&'static str, $type> = phf_map! {
                        $($name => <$type>::$value),*
                    };
                    VALUES.get(name).copied()
                }
                fn name(self) -> &'static str {
                    match self {
                        $(Self::$value => $name,)*
                        Self::Invalid => "invalid",
                    }
                }
            }
        )*
    };
}

// If you change any of these strings, make sure to update the VSCode extension in unplug-vscode/
opcode_names! {
    CmdOp {
        Abort => "abort",
        Return => "return",
        Goto => "goto",
        Set => "set",
        If => "if",
        Elif => "elif",
        EndIf => "endif",
        Case => "case",
        Expr => "expr",
        While => "while",
        Break => "break",
        Run => "run",
        Lib => "lib",
        PushBp => "pushbp",
        PopBp => "popbp",
        SetSp => "setsp",
        Anim => "anim",
        Anim1 => "anim1",
        Anim2 => "anim2",
        Attach => "attach",
        Born => "born",
        Call => "call",
        Camera => "camera",
        Check => "check",
        Color => "color",
        Detach => "detach",
        Dir => "dir",
        MDir => "mdir",
        Disp => "disp",
        Kill => "kill",
        Light => "light",
        Menu => "menu",
        Move => "move",
        MoveTo => "moveto",
        Msg => "msg",
        Pos => "pos",
        PrintF => "printf",
        Ptcl => "ptcl",
        Read => "read",
        Scale => "scale",
        MScale => "mscale",
        Scrn => "scrn",
        Select => "select",
        Sfx => "sfx",
        Timer => "timer",
        Wait => "wait",
        Warp => "warp",
        Win => "win",
        Movie => "movie",
    }

    ExprOp {
        Equal => "eq",
        NotEqual => "ne",
        Less => "lt",
        LessEqual => "le",
        Greater => "gt",
        GreaterEqual => "ge",
        Not => "not",
        Add => "add",
        Subtract => "sub",
        Multiply => "mul",
        Divide => "div",
        Modulo => "mod",
        BitAnd => "and",
        BitOr => "or",
        BitXor => "xor",
        AddAssign => "adda",
        SubtractAssign => "suba",
        MultiplyAssign => "mula",
        DivideAssign => "diva",
        ModuloAssign => "moda",
        BitAndAssign => "anda",
        BitOrAssign => "ora",
        BitXorAssign => "xora",
        Imm16 => "i16",
        Imm32 => "i32",
        AddressOf => "addr",
        Stack => "sp",
        ParentStack => "bp",
        Flag => "flag",
        Variable => "var",
        Result1 => "result",
        Result2 => "result2",
        Pad => "pad",
        Battery => "battery",
        Money => "money",
        Item => "item",
        Atc => "atc",
        Rank => "rank",
        Exp => "exp",
        Level => "level",
        Hold => "hold",
        Map => "map",
        ActorName => "actor_name",
        ItemName => "item_name",
        Time => "time",
        CurrentSuit => "cur_suit",
        Scrap => "scrap",
        CurrentAtc => "cur_atc",
        Use => "use",
        Hit => "hit",
        StickerName => "sticker_name",
        Obj => "obj",
        Random => "rand",
        Sin => "sin",
        Cos => "cos",
        ArrayElement => "array",
    }

    TypeOp {
        Time => "@time",
        Fade => "@fade",
        Wipe => "@wipe",
        Unk203 => "@unk203",
        Anim => "@anim",
        Dir => "@dir",
        Move => "@move",
        Pos => "@pos",
        Obj => "@obj",
        Unk209 => "@unk209",
        Unk210 => "@unk210",
        Unk211 => "@unk211",
        PosX => "@pos_x",
        PosY => "@pos_y",
        PosZ => "@pos_z",
        BoneX => "@bone_x",
        BoneY => "@bone_y",
        BoneZ => "@bone_z",
        DirTo => "@dir_to",
        Color => "@color",
        Lead => "@lead",
        Sfx => "@sfx",
        Modulate => "@modulate",
        Blend => "@blend",
        Real => "@real",
        Cam => "@cam",
        Hud => "@hud",
        Unk227 => "@unk227",
        Distance => "@distance",
        Unk229 => "@unk229",
        Unk230 => "@unk230",
        Unk231 => "@unk231",
        Unk232 => "@unk232",
        Read => "@read",
        ZBlur => "@zblur",
        Unk235 => "@unk235",
        Unk236 => "@unk236",
        Unk237 => "@unk237",
        Unk238 => "@unk238",
        Letterbox => "@letterbox",
        Unk240 => "@unk240",
        Shake => "@shake",
        Mono => "@mono",
        Unk243 => "@unk243",
        Scale => "@scale",
        Cue => "@cue",
        Unk246 => "@unk246",
        Unk247 => "@unk247",
        Unk248 => "@unk248",
        Unk249 => "@unk249",
        Unk250 => "@unk250",
        Unk251 => "@unk251",
        Unk252 => "@unk252",
    }

    AsmMsgOp {
        Speed => "speed",
        Wait => "wait",
        Anim => "anim",
        Sfx => "sfx",
        Voice => "voice",
        Default => "def",
        Format => "format",
        Size => "size",
        Color => "color",
        Rgba => "rgba",
        Proportional => "prop",
        Icon => "icon",
        Shake => "shake",
        Center => "center",
        Rotate => "rotate",
        Scale => "scale",
        NumInput => "input",
        Question => "ask",
        Stay => "stay",
        Text => "text",
    }

    DirOp {
        Globals => ".globals",
        Stage => ".stage",
        Byte => ".db",
        Word => ".dw",
        Dword => ".dd",
        Lib => ".lib",
        Prologue => ".prologue",
        Startup => ".startup",
        Dead => ".dead",
        Pose => ".pose",
        TimeCycle => ".time_cycle",
        TimeUp => ".time_up",
        Interact => ".interact",
    }
}
