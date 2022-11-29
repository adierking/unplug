use super::expr::{self, Expr, SetExpr, SoundExpr};
use super::msg::{self, MsgArgs};
use super::opcodes::{CmdOp, TypeOp};
use super::pointer::Pointer;
use super::serialize::{
    self, DeserializeEvent, EventDeserializer, EventSerializer, SerializeEvent,
};
use crate::common::Text;
use std::convert::{TryFrom, TryInto};
use std::fmt;
use thiserror::Error;
use unplug_proc::{DeserializeEvent, SerializeEvent};

/// The result type for command operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for command operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    Expr(Box<expr::Error>),

    #[error(transparent)]
    Msg(Box<msg::Error>),

    #[error(transparent)]
    Serialize(Box<serialize::Error>),
}

from_error_boxed!(Error::Expr, expr::Error);
from_error_boxed!(Error::Msg, msg::Error);
from_error_boxed!(Error::Serialize, serialize::Error);

/// A command in an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Abort,
    Return,
    Goto(Pointer),
    Set(Box<SetArgs>),
    If(Box<IfArgs>),
    Elif(Box<IfArgs>),
    EndIf(Pointer),
    Case(Box<IfArgs>),
    Expr(Box<IfArgs>),
    While(Box<IfArgs>),
    Break(Pointer),
    Run(Pointer),
    Lib(i16),
    PushBp,
    PopBp,
    SetSp(Box<Expr>),
    Anim(Box<AnimArgs>),
    Anim1(Box<AnimArgs>),
    Anim2(Box<AnimArgs>),
    Attach(Box<AttachArgs>),
    Born(Box<BornArgs>),
    Call(Box<CallArgs>),
    Camera(Box<CameraType>),
    Check(Box<CheckType>),
    Color(Box<ColorArgs>),
    Detach(Box<Expr>),
    Dir(Box<DirArgs>),
    MDir(Box<MDirArgs>),
    Disp(Box<DispArgs>),
    Kill(Box<Expr>),
    Light(Box<LightArgs>),
    Menu(Box<MenuType>),
    Move(Box<MoveArgs>),
    MoveTo(Box<MoveToArgs>),
    Msg(Box<MsgArgs>),
    Pos(Box<PosArgs>),
    PrintF(Box<PrintFArgs>),
    Ptcl(Box<PtclArgs>),
    Read(Box<ReadType>),
    Scale(Box<ScaleArgs>),
    MScale(Box<MScaleArgs>),
    Scrn(Box<ScrnType>),
    Select(Box<MsgArgs>),
    Sfx(Box<SfxArgs>),
    Timer(Box<TimerArgs>),
    Wait(Box<CheckType>),
    Warp(Box<WarpArgs>),
    Win(Box<WinType>),
    Movie(Box<MovieArgs>),
}

impl Command {
    /// Returns `true` if the command is an `if` statement.
    #[must_use]
    pub fn is_if(&self) -> bool {
        self.if_args().is_some()
    }

    /// If the command is an `if` statement, retrieve its `IfArgs`.
    #[must_use]
    pub fn if_args(&self) -> Option<&IfArgs> {
        match self {
            Self::If(x) => Some(x),
            Self::Elif(x) => Some(x),
            Self::Case(x) => Some(x),
            Self::Expr(x) => Some(x),
            Self::While(x) => Some(x),
            _ => None,
        }
    }

    /// If the command is an `if` statement, retrieve a mutable reference to its `IfArgs`.
    #[must_use]
    pub fn if_args_mut(&mut self) -> Option<&mut IfArgs> {
        match self {
            Self::If(x) => Some(x),
            Self::Elif(x) => Some(x),
            Self::Case(x) => Some(x),
            Self::Expr(x) => Some(x),
            Self::While(x) => Some(x),
            _ => None,
        }
    }

    /// Returns `true` if the command always jumps to another offset.
    #[must_use]
    pub fn is_goto(&self) -> bool {
        self.goto_target().is_some()
    }

    /// If the command always jumps to another offset, retrieve the target.
    #[must_use]
    pub fn goto_target(&self) -> Option<&Pointer> {
        match self {
            Self::Break(x) => Some(x),
            Self::EndIf(x) => Some(x),
            Self::Goto(x) => Some(x),
            _ => None,
        }
    }

    /// If the command always jumps to another offset, retrieve a mutable reference to the target.
    #[must_use]
    pub fn goto_target_mut(&mut self) -> Option<&mut Pointer> {
        match self {
            Self::Break(x) => Some(x),
            Self::EndIf(x) => Some(x),
            Self::Goto(x) => Some(x),
            _ => None,
        }
    }

    /// Returns `true` if the command may jump to another offset or end the event. Function calls are
    /// not included.
    #[must_use]
    pub fn is_control_flow(&self) -> bool {
        matches!(
            self,
            Self::Abort
                | Self::Return
                | Self::Goto(_)
                | Self::If(_)
                | Self::Elif(_)
                | Self::EndIf(_)
                | Self::Case(_)
                | Self::Expr(_)
                | Self::While(_)
                | Self::Break(_)
        )
    }

    /// Returns the opcode corresponding to the command.
    #[must_use]
    pub fn opcode(&self) -> CmdOp {
        match self {
            Self::Abort => CmdOp::Abort,
            Self::Return => CmdOp::Return,
            Self::Goto(_) => CmdOp::Goto,
            Self::Set(_) => CmdOp::Set,
            Self::If(_) => CmdOp::If,
            Self::Elif(_) => CmdOp::Elif,
            Self::EndIf(_) => CmdOp::EndIf,
            Self::Case(_) => CmdOp::Case,
            Self::Expr(_) => CmdOp::Expr,
            Self::While(_) => CmdOp::While,
            Self::Break(_) => CmdOp::Break,
            Self::Run(_) => CmdOp::Run,
            Self::Lib(_) => CmdOp::Lib,
            Self::PushBp => CmdOp::PushBp,
            Self::PopBp => CmdOp::PopBp,
            Self::SetSp(_) => CmdOp::SetSp,
            Self::Anim(_) => CmdOp::Anim,
            Self::Anim1(_) => CmdOp::Anim1,
            Self::Anim2(_) => CmdOp::Anim2,
            Self::Attach(_) => CmdOp::Attach,
            Self::Born(_) => CmdOp::Born,
            Self::Call(_) => CmdOp::Call,
            Self::Camera(_) => CmdOp::Camera,
            Self::Check(_) => CmdOp::Check,
            Self::Color(_) => CmdOp::Color,
            Self::Detach(_) => CmdOp::Detach,
            Self::Dir(_) => CmdOp::Dir,
            Self::MDir(_) => CmdOp::MDir,
            Self::Disp(_) => CmdOp::Disp,
            Self::Kill(_) => CmdOp::Kill,
            Self::Light(_) => CmdOp::Light,
            Self::Menu(_) => CmdOp::Menu,
            Self::Move(_) => CmdOp::Move,
            Self::MoveTo(_) => CmdOp::MoveTo,
            Self::Msg(_) => CmdOp::Msg,
            Self::Pos(_) => CmdOp::Pos,
            Self::PrintF(_) => CmdOp::PrintF,
            Self::Ptcl(_) => CmdOp::Ptcl,
            Self::Read(_) => CmdOp::Read,
            Self::Scale(_) => CmdOp::Scale,
            Self::MScale(_) => CmdOp::MScale,
            Self::Scrn(_) => CmdOp::Scrn,
            Self::Select(_) => CmdOp::Select,
            Self::Sfx(_) => CmdOp::Sfx,
            Self::Timer(_) => CmdOp::Timer,
            Self::Wait(_) => CmdOp::Wait,
            Self::Warp(_) => CmdOp::Warp,
            Self::Win(_) => CmdOp::Win,
            Self::Movie(_) => CmdOp::Movie,
        }
    }
}

impl DeserializeEvent for Command {
    type Error = Error;
    fn deserialize(de: &mut dyn EventDeserializer) -> Result<Self> {
        let cmd = de.begin_command()?;
        let result = match cmd {
            CmdOp::Abort => Self::Abort,
            CmdOp::Return => Self::Return,
            CmdOp::Goto => Self::Goto(Pointer::deserialize(de)?),
            CmdOp::Set => Self::Set(SetArgs::deserialize(de)?.into()),
            CmdOp::If => Self::If(IfArgs::deserialize(de)?.into()),
            CmdOp::Elif => Self::Elif(IfArgs::deserialize(de)?.into()),
            CmdOp::EndIf => Self::EndIf(Pointer::deserialize(de)?),
            CmdOp::Case => Self::Case(IfArgs::deserialize(de)?.into()),
            CmdOp::Expr => Self::Expr(IfArgs::deserialize(de)?.into()),
            CmdOp::While => Self::While(IfArgs::deserialize(de)?.into()),
            CmdOp::Break => Self::Break(Pointer::deserialize(de)?),
            CmdOp::Run => Self::Run(Pointer::deserialize(de)?),
            CmdOp::Lib => Self::Lib(de.deserialize_i16()?),
            CmdOp::PushBp => Self::PushBp,
            CmdOp::PopBp => Self::PopBp,
            CmdOp::SetSp => Self::SetSp(Expr::deserialize(de)?.into()),
            CmdOp::Anim => Self::Anim(AnimArgs::deserialize(de)?.into()),
            CmdOp::Anim1 => Self::Anim1(AnimArgs::deserialize(de)?.into()),
            CmdOp::Anim2 => Self::Anim2(AnimArgs::deserialize(de)?.into()),
            CmdOp::Attach => Self::Attach(AttachArgs::deserialize(de)?.into()),
            CmdOp::Born => Self::Born(BornArgs::deserialize(de)?.into()),
            CmdOp::Call => Self::Call(CallArgs::deserialize(de)?.into()),
            CmdOp::Camera => Self::Camera(CameraType::deserialize(de)?.into()),
            CmdOp::Check => Self::Check(CheckType::deserialize(de)?.into()),
            CmdOp::Color => Self::Color(ColorArgs::deserialize(de)?.into()),
            CmdOp::Detach => Self::Detach(Expr::deserialize(de)?.into()),
            CmdOp::Dir => Self::Dir(DirArgs::deserialize(de)?.into()),
            CmdOp::MDir => Self::MDir(MDirArgs::deserialize(de)?.into()),
            CmdOp::Disp => Self::Disp(DispArgs::deserialize(de)?.into()),
            CmdOp::Kill => Self::Kill(Expr::deserialize(de)?.into()),
            CmdOp::Light => Self::Light(LightArgs::deserialize(de)?.into()),
            CmdOp::Menu => Self::Menu(MenuType::deserialize(de)?.into()),
            CmdOp::Move => Self::Move(MoveArgs::deserialize(de)?.into()),
            CmdOp::MoveTo => Self::MoveTo(MoveToArgs::deserialize(de)?.into()),
            CmdOp::Msg => Self::Msg(MsgArgs::deserialize(de)?.into()),
            CmdOp::Pos => Self::Pos(PosArgs::deserialize(de)?.into()),
            CmdOp::PrintF => Self::PrintF(PrintFArgs::deserialize(de)?.into()),
            CmdOp::Ptcl => Self::Ptcl(PtclArgs::deserialize(de)?.into()),
            CmdOp::Read => Self::Read(ReadType::deserialize(de)?.into()),
            CmdOp::Scale => Self::Scale(ScaleArgs::deserialize(de)?.into()),
            CmdOp::MScale => Self::MScale(MScaleArgs::deserialize(de)?.into()),
            CmdOp::Scrn => Self::Scrn(ScrnType::deserialize(de)?.into()),
            CmdOp::Select => Self::Select(MsgArgs::deserialize(de)?.into()),
            CmdOp::Sfx => Self::Sfx(SfxArgs::deserialize(de)?.into()),
            CmdOp::Timer => Self::Timer(TimerArgs::deserialize(de)?.into()),
            CmdOp::Wait => Self::Wait(CheckType::deserialize(de)?.into()),
            CmdOp::Warp => Self::Warp(WarpArgs::deserialize(de)?.into()),
            CmdOp::Win => Self::Win(WinType::deserialize(de)?.into()),
            CmdOp::Movie => Self::Movie(MovieArgs::deserialize(de)?.into()),
        };
        de.end_command()?;
        Ok(result)
    }
}

impl SerializeEvent for Command {
    type Error = Error;
    fn serialize(&self, ser: &mut dyn EventSerializer) -> Result<()> {
        ser.begin_command(self.opcode())?;
        match self {
            Self::Abort => (),
            Self::Return => (),
            Self::Goto(ptr) => ptr.serialize(ser)?,
            Self::Set(arg) => arg.serialize(ser)?,
            Self::If(arg) => arg.serialize(ser)?,
            Self::Elif(arg) => arg.serialize(ser)?,
            Self::EndIf(ptr) => ptr.serialize(ser)?,
            Self::Case(arg) => arg.serialize(ser)?,
            Self::Expr(arg) => arg.serialize(ser)?,
            Self::While(arg) => arg.serialize(ser)?,
            Self::Break(ptr) => ptr.serialize(ser)?,
            Self::Run(ptr) => ptr.serialize(ser)?,
            Self::Lib(index) => index.serialize(ser)?,
            Self::PushBp => (),
            Self::PopBp => (),
            Self::SetSp(arg) => arg.serialize(ser)?,
            Self::Anim(arg) => arg.serialize(ser)?,
            Self::Anim1(arg) => arg.serialize(ser)?,
            Self::Anim2(arg) => arg.serialize(ser)?,
            Self::Attach(arg) => arg.serialize(ser)?,
            Self::Born(arg) => arg.serialize(ser)?,
            Self::Call(arg) => arg.serialize(ser)?,
            Self::Camera(arg) => arg.serialize(ser)?,
            Self::Check(arg) => arg.serialize(ser)?,
            Self::Color(arg) => arg.serialize(ser)?,
            Self::Detach(arg) => arg.serialize(ser)?,
            Self::Dir(arg) => arg.serialize(ser)?,
            Self::MDir(arg) => arg.serialize(ser)?,
            Self::Disp(arg) => arg.serialize(ser)?,
            Self::Kill(arg) => arg.serialize(ser)?,
            Self::Light(arg) => arg.serialize(ser)?,
            Self::Menu(arg) => arg.serialize(ser)?,
            Self::Move(arg) => arg.serialize(ser)?,
            Self::MoveTo(arg) => arg.serialize(ser)?,
            Self::Msg(arg) => arg.serialize(ser)?,
            Self::Pos(arg) => arg.serialize(ser)?,
            Self::PrintF(arg) => arg.serialize(ser)?,
            Self::Ptcl(arg) => arg.serialize(ser)?,
            Self::Read(arg) => arg.serialize(ser)?,
            Self::Scale(arg) => arg.serialize(ser)?,
            Self::MScale(arg) => arg.serialize(ser)?,
            Self::Scrn(arg) => arg.serialize(ser)?,
            Self::Select(arg) => arg.serialize(ser)?,
            Self::Sfx(arg) => arg.serialize(ser)?,
            Self::Timer(arg) => arg.serialize(ser)?,
            Self::Wait(arg) => arg.serialize(ser)?,
            Self::Warp(arg) => arg.serialize(ser)?,
            Self::Win(arg) => arg.serialize(ser)?,
            Self::Movie(arg) => arg.serialize(ser)?,
        }
        Ok(ser.end_command()?)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SetArgs {
    pub value: Expr,
    pub target: SetExpr,
}

impl SetArgs {
    /// Constructs a new `SetArgs` from a target and a value.
    pub fn new(target: SetExpr, value: Expr) -> Self {
        Self { value, target }
    }
}

impl From<SetArgs> for Command {
    fn from(args: SetArgs) -> Self {
        Self::Set(args.into())
    }
}

impl DeserializeEvent for SetArgs {
    type Error = Error;
    fn deserialize(de: &mut dyn EventDeserializer) -> Result<Self> {
        // set() has an optimization where in-place assignments (e.g. `a += b`) only store one
        // expression instead of two. The expression for the assignment target is interpreted as
        // both an Expr and an SetExpr.
        let value = Expr::deserialize(de)?;
        let target = if value.is_assign() {
            value.lhs().unwrap().clone().try_into()?
        } else {
            SetExpr::deserialize(de)?
        };
        Ok(Self { value, target })
    }
}

impl SerializeEvent for SetArgs {
    type Error = Error;
    fn serialize(&self, ser: &mut dyn EventSerializer) -> Result<()> {
        self.value.serialize(ser)?;
        // See deserialize()
        if !self.value.is_assign() {
            self.target.serialize(ser)?;
        }
        Ok(())
    }
}

// Custom Debug impl to print the target first
impl fmt::Debug for SetArgs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SetArgs").field("target", &self.target).field("value", &self.value).finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct IfArgs {
    pub condition: Expr,
    pub else_target: Pointer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimArgs {
    pub obj: Expr,
    pub values: Vec<Expr>,
}

impl DeserializeEvent for AnimArgs {
    type Error = Error;
    fn deserialize(de: &mut dyn EventDeserializer) -> Result<Self> {
        let obj = Expr::deserialize(de)?;
        let mut values = vec![];
        loop {
            let val = Expr::deserialize(de)?;
            if let Some(-1) = val.value() {
                break;
            }
            values.push(val);
        }
        Ok(Self { obj, values })
    }
}

impl SerializeEvent for AnimArgs {
    type Error = Error;
    fn serialize(&self, ser: &mut dyn EventSerializer) -> Result<()> {
        self.obj.serialize(ser)?;
        for val in &self.values {
            val.serialize(ser)?;
        }
        Expr::Imm32(-1).serialize(ser)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct AttachArgs {
    pub obj: Expr,
    pub event: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct BornArgs {
    pub val1: Expr,
    pub val2: Expr,
    pub val3: Expr,
    pub val4: Expr,
    pub val5: Expr,
    pub val6: Expr,
    pub val7: Expr,
    pub val8: Expr,
    pub val9: Expr,
    pub event: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallArgs {
    pub obj: Expr,
    pub args: Vec<Expr>,
}

impl DeserializeEvent for CallArgs {
    type Error = Error;
    fn deserialize(de: &mut dyn EventDeserializer) -> Result<Self> {
        de.begin_call()?;
        let obj = Expr::deserialize(de)?;
        let mut args = vec![];
        while de.have_call_arg()? {
            args.push(Expr::deserialize(de)?);
        }
        de.end_call()?;
        Ok(Self { obj, args })
    }
}

impl SerializeEvent for CallArgs {
    type Error = Error;
    fn serialize(&self, ser: &mut dyn EventSerializer) -> Result<()> {
        ser.begin_call()?;
        self.obj.serialize(ser)?;
        for arg in &self.args {
            arg.serialize(ser)?;
        }
        Ok(ser.end_call()?)
    }
}

expr_enum! {
    type Error = Error;
    pub enum CameraType {
        Anim(CameraAnimArgs { val1, val2, val3 }) => TypeOp::Anim,
        Pos(CameraPosArgs { val1, val2, val3, val4, val5 }) => TypeOp::Pos,
        Obj(CameraObjArgs { val1, val2, val3 }) => TypeOp::Obj,
        Unk209(CameraUnk209Args { val1, val2 }) => TypeOp::Unk209,
        Unk211(CameraUnk211Args { val1, val2, val3, val4 }) => TypeOp::Unk211,
        Lead(CameraLeadArgs { val }) => TypeOp::Lead,
        Unk227(CameraUnk227Args { val1, val2, val3, val4, val5 }) => TypeOp::Unk227,
        Unk228(CameraDistanceArgs { val1, val2, val3 }) => TypeOp::Distance,
        Unk229(CameraUnk229Args { val1, val2, val3 }) => TypeOp::Unk229,
        Unk230 => TypeOp::Unk230,
        Unk232(CameraUnk232Type) => TypeOp::Unk232,
        Unk236(CameraUnk236Args { val }) => TypeOp::Unk236,
        Unk237(CameraUnk237Args { val }) => TypeOp::Unk237,
        Unk238(CameraUnk238Args { val }) => TypeOp::Unk238,
        Unk240(CameraUnk240Args { val1, val2, val3, val4 }) => TypeOp::Unk240,
        Unk243(CameraUnk243Args { val1, val2, val3, val4 }) => TypeOp::Unk243,
        Unk251(CameraUnk251Args { val1, val2, val3, val4 }) => TypeOp::Unk251,
        Unk252(CameraUnk252Args { val1, val2, val3, val4 }) => TypeOp::Unk252,
    }
}

expr_enum! {
    type Error = Error;
    pub enum CameraUnk232Type {
        UnkN2 => -2,
        UnkN1 => -1,
        Unk0 => 0,
        Unk1 => 1,
        Unk2(CameraUnk232Unk2Args { val }) => 2,
        Unk3(CameraUnk232Unk3Args { val }) => 3,
        Unk4(CameraUnk232Unk4Args { val }) => 4,
    }
}

expr_enum! {
    type Error = Error;
    pub enum CheckType {
        Time(CheckTimeArgs { duration }) => TypeOp::Time,
        Fade => TypeOp::Fade,
        Wipe => TypeOp::Wipe,
        Unk203 => TypeOp::Unk203,
        Anim(CheckAnimArgs { obj, val }) => TypeOp::Anim,
        Dir(CheckDirArgs { obj }) => TypeOp::Dir,
        Move(CheckMoveArgs { obj }) => TypeOp::Move,
        Color(CheckColorArgs { obj }) => TypeOp::Color,
        Sfx(CheckSfxArgs) => TypeOp::Sfx,
        Real(CheckRealArgs { val }) => TypeOp::Real,
        Cam => TypeOp::Cam,
        Read(CheckReadArgs { obj }) => TypeOp::Read,
        ZBlur => TypeOp::ZBlur,
        Letterbox => TypeOp::Letterbox,
        Shake => TypeOp::Shake,
        Mono => TypeOp::Mono,
        Scale(CheckScaleArgs { obj }) => TypeOp::Scale,
        Cue => TypeOp::Cue,
        Unk246(CheckUnk246Args { val }) => TypeOp::Unk246,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct CheckSfxArgs {
    sfx: SoundExpr,
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct ColorArgs {
    pub obj: Expr,
    pub ty: ColorType,
    pub val1: Expr,
    pub val2: Expr,
    pub val3: Expr,
    pub val4: Expr,
}

expr_enum! {
    type Error = Error;
    pub enum ColorType {
        Modulate => TypeOp::Modulate,
        Blend => TypeOp::Blend,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct DirArgs {
    pub obj: Expr,
    pub val: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct MDirArgs {
    pub obj: Expr,
    pub ty: MDirType,
    pub val1: Expr,
    pub val2: Expr,
}

expr_enum! {
    type Error = Error;
    pub enum MDirType {
        Dir(MDirDirArgs { val }) => TypeOp::Dir,
        Pos(MDirPosArgs { val1, val2 }) => TypeOp::Pos,
        Obj(MDirObjArgs { val }) => TypeOp::Obj,
        Cam => TypeOp::Cam,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct DispArgs {
    pub obj: Expr,
    pub disp: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct LightArgs {
    pub val: Expr,
    pub ty: LightType,
}

expr_enum! {
    type Error = Error;
    pub enum LightType {
        Pos(LightPosArgs { x, y, z }) => TypeOp::Pos,
        Color(LightColorArgs { r, g, b }) => TypeOp::Color,
        Unk227(LightUnk227Args { val1, val2, val3 }) => TypeOp::Unk227,
    }
}

expr_enum! {
    type Error = Error;
    pub enum MenuType {
        Main => 0,
        Status => 1,
        Item => 2,
        Pc => 3,
        Save => 4,
        StageSelect => 5,
        McTest => 6,
        McTestSub => 7,
        Unk1000(MenuUnk1000Args { val }) => 1000,
        Unk1001(MenuUnk1001Args { val1, val2 }) => 1001,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct MoveArgs {
    pub obj: Expr,
    pub val1: Expr,
    pub val2: Expr,
    pub val3: Expr,
    pub val4: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct MoveToArgs {
    pub obj: Expr,
    pub val1: Expr,
    pub val2: Expr,
    pub val3: Expr,
    pub val4: Expr,
    pub val5: Expr,
    pub val6: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct PosArgs {
    pub obj: Expr,
    pub x: Expr,
    pub y: Expr,
    pub z: Expr,
}

#[derive(Clone, PartialEq, Eq)]
pub struct PrintFArgs(pub Text);

impl DeserializeEvent for PrintFArgs {
    type Error = serialize::Error;
    fn deserialize(de: &mut dyn EventDeserializer) -> serialize::Result<Self> {
        Ok(Self(de.deserialize_text()?))
    }
}

impl SerializeEvent for PrintFArgs {
    type Error = serialize::Error;
    fn serialize(&self, ser: &mut dyn EventSerializer) -> serialize::Result<()> {
        self.0.serialize(ser)
    }
}

impl fmt::Debug for PrintFArgs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct PtclArgs {
    pub val: Expr,
    pub ty: PtclType,
}

expr_enum! {
    type Error = Error;
    pub enum PtclType {
        Pos(PtclPosArgs { val1, val2, val3, val4, val5, val6, val7 }) => TypeOp::Pos,
        Obj(PtclObjArgs { obj, val1, val2, val3, val4, val5, val6, val7 }) => TypeOp::Obj,
        Unk210 => TypeOp::Unk210,
        Lead(PtclLeadArgs) => TypeOp::Lead,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtclLeadArgs {
    obj: Expr,
    args: Vec<Expr>,
}

impl DeserializeEvent for PtclLeadArgs {
    type Error = Error;
    fn deserialize(de: &mut dyn EventDeserializer) -> Result<Self> {
        let obj = Expr::deserialize(de)?;
        let argc_expr = Expr::deserialize(de)?;
        let argc = argc_expr.value().ok_or_else(|| expr::Error::NonConstant(argc_expr.opcode()))?;
        let mut args = vec![];
        for _ in 0..argc {
            args.push(Expr::deserialize(de)?);
        }
        Ok(Self { obj, args })
    }
}

impl SerializeEvent for PtclLeadArgs {
    type Error = Error;
    fn serialize(&self, ser: &mut dyn EventSerializer) -> Result<()> {
        self.obj.serialize(ser)?;
        let argc = i32::try_from(self.args.len()).expect("PtclLeadArgs size overflow");
        Expr::Imm32(argc).serialize(ser)?;
        for arg in &self.args {
            arg.serialize(ser)?;
        }
        Ok(())
    }
}

expr_enum! {
    type Error = Error;
    pub enum ReadType {
        Anim(ReadAnimArgs { obj, path }) => TypeOp::Anim,
        Sfx(ReadSfxArgs { obj, path }) => TypeOp::Sfx,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct ScaleArgs {
    pub obj: Expr,
    pub x: Expr,
    pub y: Expr,
    pub z: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct MScaleArgs {
    pub obj: Expr,
    pub x: Expr,
    pub y: Expr,
    pub z: Expr,
    pub val: Expr,
}

expr_enum! {
    type Error = Error;
    pub enum ScrnType {
        Fade(ScrnFadeArgs {
            from_r, from_g, from_b, from_a, to_r, to_g, to_b, to_a, duration
        }) => TypeOp::Fade,
        Wipe(ScrnWipeArgs {
            // valid modes: 0 (circle), 1 (square), 1000 (???), 2000 (many circles)
            mode, direction, x, y, from_width, from_height, from_r, from_g, from_b, from_a,
            to_width, to_height, to_r, to_g, to_b, to_a, duration
        }) => TypeOp::Wipe,
        Hud(ScrnHudType) => TypeOp::Hud,
        ZBlur(ScrnZBlurArgs { from, to, x_scale, y_scale, duration }) => TypeOp::ZBlur,
        Letterbox(ScrnLetterboxArgs {
            direction, from_r, from_g, from_b, from_a, to_r, to_g, to_b, to_a, duration
        }) => TypeOp::Letterbox,
        Shake(ScrnShakeArgs { from_x, from_y, from_z, to_x, to_y, to_z, duration }) => TypeOp::Shake,
        Mono(ScrnMonoArgs {
            from_r, from_g, from_b, from_a, to_r, to_g, to_b, to_a, duration
        }) => TypeOp::Mono,
    }
}

expr_enum! {
    type Error = Error;
    #[allow(variant_size_differences)]
    pub enum ScrnHudType {
        Battery(ScrnBatteryArgs { command }) => 0,
        Clock(ScrnClockArgs { command }) => 1,
        Score(ScrnScoreArgs { command }) => 2,
        Unk3(ScrnHudUnk3Args { val1, val2, val3, val4 }) => 3,
        Timer(ScrnTimerCommand) => 4,
    }
}

expr_enum! {
    type Error = Error;
    pub enum ScrnTimerCommand {
        Get => -4,
        Set(Expr) => -3,
        Pause => -2,
        Unpause => -1,
        Show => 0,
        Hide => 1,
        FadeIn => 2,
        FadeOut => 3,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct SfxArgs {
    pub sfx: SoundExpr,
    pub ty: SfxType,
}

expr_enum! {
    type Error = Error;
    pub enum SfxType {
        Stop => 0,
        Play => 1,
        FadeOut(SfxFadeOutArgs { duration }) => 2,
        FadeIn(SfxFadeInArgs { duration }) => 3,
        Fade(SfxFadeArgs { duration, volume }) => 4,
        Unk5 => 5,
        Unk6 => 6,
        Unk245 => TypeOp::Cue,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct TimerArgs {
    pub duration: Expr,
    pub event: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct WarpArgs {
    pub stage: Expr,
    pub val: Expr,
}

expr_enum! {
    type Error = Error;
    pub enum WinType {
        Pos(WinPosArgs { val1, val2 }) => TypeOp::Pos,
        Obj(WinObjArgs { obj, val1, val2, val3 }) => TypeOp::Obj,
        Unk209 => TypeOp::Unk209,
        Color(WinColorArgs { val1, val2, val3, val4 }) => TypeOp::Color,
        Letterbox => TypeOp::Letterbox,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
#[serialize(error = Error)]
pub struct MovieArgs {
    pub path: Expr,
    pub val1: Expr,
    pub val2: Expr,
    pub val3: Expr,
    pub val4: Expr,
    pub val5: Expr,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_reserialize;
    use crate::common::Text;
    use crate::data::Music;
    use crate::event::expr::BinaryOp;
    use crate::event::msg::MsgCommand;

    fn expr() -> Expr {
        Expr::Imm32(123)
    }

    fn ptr() -> Pointer {
        Pointer::Offset(123)
    }

    fn binary_op(lhs: Expr, rhs: Expr) -> Box<BinaryOp> {
        Box::new(BinaryOp { rhs, lhs })
    }

    fn if_args() -> Box<IfArgs> {
        Box::new(IfArgs { condition: expr(), else_target: ptr() })
    }

    fn text(string: &str) -> Text {
        Text::encode(string).unwrap()
    }

    #[test]
    fn test_reserialize_command() {
        assert_reserialize!(Command::Abort);
        assert_reserialize!(Command::Return);
        assert_reserialize!(Command::Goto(ptr()));
        assert_reserialize!(Command::Set(Box::new(SetArgs {
            target: SetExpr::from_var(123),
            value: expr(),
        })));
        assert_reserialize!(Command::Set(Box::new(SetArgs {
            target: SetExpr::from_var(123),
            value: Expr::AddAssign(binary_op(Expr::from_var(123), expr())),
        })));
        assert_reserialize!(Command::If(if_args()));
        assert_reserialize!(Command::Elif(if_args()));
        assert_reserialize!(Command::EndIf(ptr()));
        assert_reserialize!(Command::Case(if_args()));
        assert_reserialize!(Command::Expr(if_args()));
        assert_reserialize!(Command::While(if_args()));
        assert_reserialize!(Command::Break(ptr()));
        assert_reserialize!(Command::Run(ptr()));
        assert_reserialize!(Command::Lib(123));
        assert_reserialize!(Command::PushBp);
        assert_reserialize!(Command::PopBp);
        assert_reserialize!(Command::SetSp(Box::new(expr())));
        assert_reserialize!(Command::Anim(Box::new(AnimArgs {
            obj: expr(),
            values: vec![expr(), expr(), expr(), expr(), expr(), expr()],
        })));
        assert_reserialize!(Command::Anim1(Box::new(AnimArgs {
            obj: expr(),
            values: vec![expr(), expr(), expr(), expr(), expr(), expr()],
        })));
        assert_reserialize!(Command::Anim2(Box::new(AnimArgs {
            obj: expr(),
            values: vec![expr(), expr(), expr(), expr(), expr(), expr()],
        })));
        assert_reserialize!(Command::Attach(Box::new(AttachArgs { obj: expr(), event: expr() })));
        assert_reserialize!(Command::Born(Box::new(BornArgs {
            val1: expr(),
            val2: expr(),
            val3: expr(),
            val4: expr(),
            val5: expr(),
            val6: expr(),
            val7: expr(),
            val8: expr(),
            val9: expr(),
            event: expr(),
        })));
        assert_reserialize!(Command::Call(Box::new(CallArgs {
            obj: expr(),
            args: vec![expr(), expr(), expr()],
        })));
        assert_reserialize!(Command::Camera(Box::new(CameraType::Anim(CameraAnimArgs {
            val1: expr(),
            val2: expr(),
            val3: expr(),
        }))));
        assert_reserialize!(Command::Check(Box::new(CheckType::Scale(CheckScaleArgs {
            obj: expr(),
        }))));
        assert_reserialize!(Command::Color(Box::new(ColorArgs {
            obj: expr(),
            ty: ColorType::Modulate,
            val1: expr(),
            val2: expr(),
            val3: expr(),
            val4: expr(),
        })));
        assert_reserialize!(Command::Detach(Box::new(expr())));
        assert_reserialize!(Command::Dir(Box::new(DirArgs { obj: expr(), val: expr() })));
        assert_reserialize!(Command::MDir(Box::new(MDirArgs {
            obj: expr(),
            ty: MDirType::Dir(MDirDirArgs { val: expr() }),
            val1: expr(),
            val2: expr(),
        })));
        assert_reserialize!(Command::Disp(Box::new(DispArgs { obj: expr(), disp: expr() })));
        assert_reserialize!(Command::Kill(Box::new(expr())));
        assert_reserialize!(Command::Light(Box::new(LightArgs {
            val: expr(),
            ty: LightType::Pos(LightPosArgs { x: expr(), y: expr(), z: expr() }),
        })));
        assert_reserialize!(Command::Menu(Box::new(MenuType::Item)));
        assert_reserialize!(Command::Move(Box::new(MoveArgs {
            obj: expr(),
            val1: expr(),
            val2: expr(),
            val3: expr(),
            val4: expr(),
        })));
        assert_reserialize!(Command::MoveTo(Box::new(MoveToArgs {
            obj: expr(),
            val1: expr(),
            val2: expr(),
            val3: expr(),
            val4: expr(),
            val5: expr(),
            val6: expr(),
        })));
        assert_reserialize!(Command::Msg(Box::new(MsgArgs::from(text("bunger")))));
        assert_reserialize!(Command::Pos(Box::new(PosArgs {
            obj: expr(),
            x: expr(),
            y: expr(),
            z: expr(),
        })));
        assert_reserialize!(Command::PrintF(Box::new(PrintFArgs(text("bunger")))));
        assert_reserialize!(Command::PrintF(Box::new(PrintFArgs(text("スプラトゥーン")))));
        assert_reserialize!(Command::Ptcl(Box::new(PtclArgs {
            val: expr(),
            ty: PtclType::Lead(PtclLeadArgs { obj: expr(), args: vec![expr(), expr(), expr()] }),
        })));
        assert_reserialize!(Command::Read(Box::new(ReadType::Anim(ReadAnimArgs {
            obj: expr(),
            path: expr(),
        }))));
        assert_reserialize!(Command::Scale(Box::new(ScaleArgs {
            obj: expr(),
            x: expr(),
            y: expr(),
            z: expr(),
        })));
        assert_reserialize!(Command::MScale(Box::new(MScaleArgs {
            obj: expr(),
            x: expr(),
            y: expr(),
            z: expr(),
            val: expr(),
        })));
        assert_reserialize!(Command::Scrn(Box::new(ScrnType::ZBlur(ScrnZBlurArgs {
            from: expr(),
            to: expr(),
            x_scale: expr(),
            y_scale: expr(),
            duration: expr(),
        }))));
        assert_reserialize!(Command::Select(Box::new(MsgArgs::from(vec![
            MsgCommand::Text(text("circle")),
            MsgCommand::Newline,
            MsgCommand::Text(text("middle")),
            MsgCommand::Newline,
            MsgCommand::Text(text("radar")),
        ]))));
        assert_reserialize!(Command::Sfx(Box::new(SfxArgs {
            sfx: SoundExpr::Music(Music::BgmNight),
            ty: SfxType::Stop
        })));
        assert_reserialize!(Command::Timer(Box::new(TimerArgs {
            duration: expr(),
            event: expr(),
        })));
        assert_reserialize!(Command::Wait(Box::new(CheckType::Scale(CheckScaleArgs {
            obj: expr(),
        }))));
        assert_reserialize!(Command::Warp(Box::new(WarpArgs { stage: expr(), val: expr() })));
        assert_reserialize!(Command::Win(Box::new(WinType::Unk209)));
        assert_reserialize!(Command::Movie(Box::new(MovieArgs {
            path: expr(),
            val1: expr(),
            val2: expr(),
            val3: expr(),
            val4: expr(),
            val5: expr(),
        })));
    }
}
