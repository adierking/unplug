use super::block::{Ip, WriteIp};
use super::expr::{self, Expr, SetExpr, SoundExpr};
use super::msg::{self, MsgArgs};
use super::opcodes::{CmdOp, Ggte, OpcodeMap, TypeOp};
use crate::common::{ReadFrom, Text, WriteTo};
use byteorder::{ReadBytesExt, WriteBytesExt, LE};
use std::convert::{TryFrom, TryInto};
use std::ffi::CString;
use std::fmt;
use std::io::{self, Read, Seek, SeekFrom, Write};
use thiserror::Error;
use unplug_proc::{ReadFrom, WriteTo};

/// The result type for command operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for command operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("unrecognized command opcode: {0}")]
    UnrecognizedOp(u8),

    #[error("unsupported command: {0:?}")]
    UnsupportedCommand(CmdOp),

    #[error(transparent)]
    Expr(Box<expr::Error>),

    #[error(transparent)]
    Msg(Box<msg::Error>),

    #[error(transparent)]
    Io(Box<io::Error>),
}

from_error_boxed!(Error::Expr, expr::Error);
from_error_boxed!(Error::Msg, msg::Error);
from_error_boxed!(Error::Io, io::Error);

/// A command in an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Abort,
    Return,
    Goto(Ip),
    Set(Box<SetArgs>),
    If(Box<IfArgs>),
    Elif(Box<IfArgs>),
    EndIf(Ip),
    Case(Box<IfArgs>),
    Expr(Box<IfArgs>),
    While(Box<IfArgs>),
    Break(Ip),
    Run(Ip),
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
    pub fn goto_target(&self) -> Option<&Ip> {
        match self {
            Self::Break(x) => Some(x),
            Self::EndIf(x) => Some(x),
            Self::Goto(x) => Some(x),
            _ => None,
        }
    }

    /// If the command always jumps to another offset, retrieve a mutable reference to the target.
    #[must_use]
    pub fn goto_target_mut(&mut self) -> Option<&mut Ip> {
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

impl<R: Read + Seek + ?Sized> ReadFrom<R> for Command {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let opcode = reader.read_u8()?;
        let cmd = Ggte::get(opcode).map_err(Error::UnrecognizedOp)?;
        Ok(match cmd {
            CmdOp::Abort => Self::Abort,
            CmdOp::Return => Self::Return,
            CmdOp::Goto => Self::Goto(Ip::read_from(reader)?),
            CmdOp::Set => Self::Set(SetArgs::read_from(reader)?.into()),
            CmdOp::If => Self::If(IfArgs::read_from(reader)?.into()),
            CmdOp::Elif => Self::Elif(IfArgs::read_from(reader)?.into()),
            CmdOp::EndIf => Self::EndIf(Ip::read_from(reader)?),
            CmdOp::Case => Self::Case(IfArgs::read_from(reader)?.into()),
            CmdOp::Expr => Self::Expr(IfArgs::read_from(reader)?.into()),
            CmdOp::While => Self::While(IfArgs::read_from(reader)?.into()),
            CmdOp::Break => Self::Break(Ip::read_from(reader)?),
            CmdOp::Run => Self::Run(Ip::read_from(reader)?),
            CmdOp::Lib => Self::Lib(reader.read_i16::<LE>()?),
            CmdOp::PushBp => Self::PushBp,
            CmdOp::PopBp => Self::PopBp,
            CmdOp::SetSp => Self::SetSp(Expr::read_from(reader)?.into()),
            CmdOp::Anim => Self::Anim(AnimArgs::read_from(reader)?.into()),
            CmdOp::Anim1 => Self::Anim1(AnimArgs::read_from(reader)?.into()),
            CmdOp::Anim2 => Self::Anim2(AnimArgs::read_from(reader)?.into()),
            CmdOp::Attach => Self::Attach(AttachArgs::read_from(reader)?.into()),
            CmdOp::Born => Self::Born(BornArgs::read_from(reader)?.into()),
            CmdOp::Call => Self::Call(CallArgs::read_from(reader)?.into()),
            CmdOp::Camera => Self::Camera(CameraType::read_from(reader)?.into()),
            CmdOp::Check => Self::Check(CheckType::read_from(reader)?.into()),
            CmdOp::Color => Self::Color(ColorArgs::read_from(reader)?.into()),
            CmdOp::Detach => Self::Detach(Expr::read_from(reader)?.into()),
            CmdOp::Dir => Self::Dir(DirArgs::read_from(reader)?.into()),
            CmdOp::MDir => Self::MDir(MDirArgs::read_from(reader)?.into()),
            CmdOp::Disp => Self::Disp(DispArgs::read_from(reader)?.into()),
            CmdOp::Kill => Self::Kill(Expr::read_from(reader)?.into()),
            CmdOp::Light => Self::Light(LightArgs::read_from(reader)?.into()),
            CmdOp::Menu => Self::Menu(MenuType::read_from(reader)?.into()),
            CmdOp::Move => Self::Move(MoveArgs::read_from(reader)?.into()),
            CmdOp::MoveTo => Self::MoveTo(MoveToArgs::read_from(reader)?.into()),
            CmdOp::Msg => Self::Msg(MsgArgs::read_from(reader)?.into()),
            CmdOp::Pos => Self::Pos(PosArgs::read_from(reader)?.into()),
            CmdOp::PrintF => Self::PrintF(PrintFArgs::read_from(reader)?.into()),
            CmdOp::Ptcl => Self::Ptcl(PtclArgs::read_from(reader)?.into()),
            CmdOp::Read => Self::Read(ReadType::read_from(reader)?.into()),
            CmdOp::Scale => Self::Scale(ScaleArgs::read_from(reader)?.into()),
            CmdOp::MScale => Self::MScale(MScaleArgs::read_from(reader)?.into()),
            CmdOp::Scrn => Self::Scrn(ScrnType::read_from(reader)?.into()),
            CmdOp::Select => Self::Select(MsgArgs::read_from(reader)?.into()),
            CmdOp::Sfx => Self::Sfx(SfxArgs::read_from(reader)?.into()),
            CmdOp::Timer => Self::Timer(TimerArgs::read_from(reader)?.into()),
            CmdOp::Wait => Self::Wait(CheckType::read_from(reader)?.into()),
            CmdOp::Warp => Self::Warp(WarpArgs::read_from(reader)?.into()),
            CmdOp::Win => Self::Win(WinType::read_from(reader)?.into()),
            CmdOp::Movie => Self::Movie(MovieArgs::read_from(reader)?.into()),
        })
    }
}

impl<W: Write + WriteIp + Seek + ?Sized> WriteTo<W> for Command {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        let opcode = Ggte::value(self.opcode()).map_err(Error::UnsupportedCommand)?;
        writer.write_u8(opcode)?;
        match self {
            Self::Abort => Ok(()),
            Self::Return => Ok(()),
            Self::Goto(ip) => Ok(ip.write_to(writer)?),
            Self::Set(arg) => arg.write_to(writer),
            Self::If(arg) => arg.write_to(writer),
            Self::Elif(arg) => arg.write_to(writer),
            Self::EndIf(ip) => Ok(ip.write_to(writer)?),
            Self::Case(arg) => arg.write_to(writer),
            Self::Expr(arg) => arg.write_to(writer),
            Self::While(arg) => arg.write_to(writer),
            Self::Break(ip) => Ok(ip.write_to(writer)?),
            Self::Run(ip) => Ok(ip.write_to(writer)?),
            Self::Lib(index) => Ok(writer.write_i16::<LE>(*index)?),
            Self::PushBp => Ok(()),
            Self::PopBp => Ok(()),
            Self::SetSp(arg) => Ok(arg.write_to(writer)?),
            Self::Anim(arg) => arg.write_to(writer),
            Self::Anim1(arg) => arg.write_to(writer),
            Self::Anim2(arg) => arg.write_to(writer),
            Self::Attach(arg) => arg.write_to(writer),
            Self::Born(arg) => arg.write_to(writer),
            Self::Call(arg) => arg.write_to(writer),
            Self::Camera(arg) => arg.write_to(writer),
            Self::Check(arg) => arg.write_to(writer),
            Self::Color(arg) => arg.write_to(writer),
            Self::Detach(arg) => Ok(arg.write_to(writer)?),
            Self::Dir(arg) => arg.write_to(writer),
            Self::MDir(arg) => arg.write_to(writer),
            Self::Disp(arg) => arg.write_to(writer),
            Self::Kill(arg) => Ok(arg.write_to(writer)?),
            Self::Light(arg) => arg.write_to(writer),
            Self::Menu(arg) => arg.write_to(writer),
            Self::Move(arg) => arg.write_to(writer),
            Self::MoveTo(arg) => arg.write_to(writer),
            Self::Msg(arg) => Ok(arg.write_to(writer)?),
            Self::Pos(arg) => arg.write_to(writer),
            Self::PrintF(arg) => Ok(arg.write_to(writer)?),
            Self::Ptcl(arg) => arg.write_to(writer),
            Self::Read(arg) => arg.write_to(writer),
            Self::Scale(arg) => arg.write_to(writer),
            Self::MScale(arg) => arg.write_to(writer),
            Self::Scrn(arg) => arg.write_to(writer),
            Self::Select(arg) => Ok(arg.write_to(writer)?),
            Self::Sfx(arg) => arg.write_to(writer),
            Self::Timer(arg) => arg.write_to(writer),
            Self::Wait(arg) => arg.write_to(writer),
            Self::Warp(arg) => arg.write_to(writer),
            Self::Win(arg) => arg.write_to(writer),
            Self::Movie(arg) => arg.write_to(writer),
        }
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

impl<R: Read + ?Sized> ReadFrom<R> for SetArgs {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        // set() has an optimization where in-place assignments (e.g. `a += b`) only store one
        // expression instead of two. The expression for the assignment target is interpreted as
        // both an Expr and an SetExpr.
        let value = Expr::read_from(reader)?;
        let target = if value.is_assign() {
            value.lhs().unwrap().clone().try_into()?
        } else {
            SetExpr::read_from(reader)?
        };
        Ok(Self { value, target })
    }
}

impl<W: Write + WriteIp + ?Sized> WriteTo<W> for SetArgs {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        self.value.write_to(writer)?;
        // See read_from()
        if !self.value.is_assign() {
            self.target.write_to(writer)?;
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

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
pub struct IfArgs {
    pub condition: Expr,
    pub else_target: Ip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimArgs {
    pub obj: Expr,
    pub values: Vec<Expr>,
}

impl<R: Read + ?Sized> ReadFrom<R> for AnimArgs {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let obj = Expr::read_from(reader)?;
        let mut values = vec![];
        loop {
            let val = Expr::read_from(reader)?;
            if let Some(-1) = val.value() {
                break;
            }
            values.push(val);
        }
        Ok(Self { obj, values })
    }
}

impl<W: Write + WriteIp + ?Sized> WriteTo<W> for AnimArgs {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        self.obj.write_to(writer)?;
        for val in &self.values {
            val.write_to(writer)?;
        }
        Expr::Imm32(-1).write_to(writer)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
pub struct AttachArgs {
    pub obj: Expr,
    pub event: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
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

impl<R: Read + Seek + ?Sized> ReadFrom<R> for CallArgs {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let start_offset = reader.seek(SeekFrom::Current(0))?;
        let command_size = reader.read_i16::<LE>()?;
        let end_offset = start_offset + command_size as u64;
        let obj = Expr::read_from(reader)?;
        let mut args = vec![];
        while reader.seek(SeekFrom::Current(0))? < end_offset {
            args.push(Expr::read_from(reader)?);
        }
        Ok(Self { obj, args })
    }
}

impl<W: Write + WriteIp + Seek + ?Sized> WriteTo<W> for CallArgs {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        // Write a command size of 0 for now
        let start_offset = writer.seek(SeekFrom::Current(0))?;
        writer.write_i16::<LE>(0)?;

        self.obj.write_to(writer)?;
        for arg in &self.args {
            arg.write_to(writer)?;
        }

        // Now go back and fill in the command size
        let end_offset = writer.seek(SeekFrom::Current(0))?;
        let command_size =
            i16::try_from(end_offset - start_offset).expect("CallArgs size overflow");
        writer.seek(SeekFrom::Start(start_offset))?;
        writer.write_i16::<LE>(command_size)?;
        writer.seek(SeekFrom::Start(end_offset))?;
        Ok(())
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
        Unk201 => TypeOp::Unk201,
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
        Unk234 => TypeOp::Unk234,
        Unk239 => TypeOp::Unk239,
        Unk241 => TypeOp::Unk241,
        Unk242 => TypeOp::Unk242,
        Scale(CheckScaleArgs { obj }) => TypeOp::Scale,
        Cue => TypeOp::Cue,
        Unk246(CheckUnk246Args { val }) => TypeOp::Unk246,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
pub struct CheckSfxArgs {
    sfx: SoundExpr,
}

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
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

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
pub struct DirArgs {
    pub obj: Expr,
    pub val: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
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

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
pub struct DispArgs {
    pub obj: Expr,
    pub disp: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
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

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
pub struct MoveArgs {
    pub obj: Expr,
    pub val1: Expr,
    pub val2: Expr,
    pub val3: Expr,
    pub val4: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
pub struct MoveToArgs {
    pub obj: Expr,
    pub val1: Expr,
    pub val2: Expr,
    pub val3: Expr,
    pub val4: Expr,
    pub val5: Expr,
    pub val6: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
pub struct PosArgs {
    pub obj: Expr,
    pub x: Expr,
    pub y: Expr,
    pub z: Expr,
}

#[derive(Clone, PartialEq, Eq)]
pub struct PrintFArgs(pub Text);

impl<R: Read + ?Sized> ReadFrom<R> for PrintFArgs {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        Ok(Self(CString::read_from(reader)?.into()))
    }
}

impl<W: Write + ?Sized> WriteTo<W> for PrintFArgs {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(self.0.as_bytes())?;
        writer.write_u8(0)?;
        Ok(())
    }
}

impl fmt::Debug for PrintFArgs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
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

impl<R: Read + ?Sized> ReadFrom<R> for PtclLeadArgs {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let obj = Expr::read_from(reader)?;
        let argc_op = Expr::read_from(reader)?;
        let argc = match argc_op.value() {
            Some(x) => x,
            None => return Err(expr::Error::NonConstant(argc_op.into()).into()),
        };
        let mut args = vec![];
        for _ in 0..argc {
            args.push(Expr::read_from(reader)?);
        }
        Ok(Self { obj, args })
    }
}

impl<W: Write + WriteIp + ?Sized> WriteTo<W> for PtclLeadArgs {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        self.obj.write_to(writer)?;
        let argc = i32::try_from(self.args.len()).expect("PtclLeadArgs size overflow");
        Expr::Imm32(argc).write_to(writer)?;
        for arg in &self.args {
            arg.write_to(writer)?;
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

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
pub struct ScaleArgs {
    pub obj: Expr,
    pub x: Expr,
    pub y: Expr,
    pub z: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
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
        Unk201(ScrnUnk201Args {
            val1, val2, val3, val4, val5, val6, val7, val8, val9
        }) => TypeOp::Unk201,
        Wipe(ScrnWipeArgs {
            val1, val2, val3, val4, val5, val6, val7, val8, val9, val10,
            val11, val12, val13, val14, val15, val16, val17
        }) => TypeOp::Wipe,
        Unk226(ScrnUnk226Type) => TypeOp::Unk226,
        Unk234(ScrnUnk234Args { val1, val2, val3, val4, val5 }) => TypeOp::Unk234,
        Unk239(ScrnUnk239Args {
            val1, val2, val3, val4, val5, val6, val7, val8, val9, val10
        }) => TypeOp::Unk239,
        Unk241(ScrnUnk241Args { val1, val2, val3, val4, val5, val6, val7 }) => TypeOp::Unk241,
        Unk242(ScrnUnk242Args {
            val1, val2, val3, val4, val5, val6, val7, val8, val9
        }) => TypeOp::Unk242,
    }
}

expr_enum! {
    type Error = Error;
    #[allow(variant_size_differences)]
    pub enum ScrnUnk226Type {
        Unk0(ScrnUnk226Unk0Args { val }) => 0,
        Unk1(ScrnUnk226Unk1Args { val }) => 1,
        Unk2(ScrnUnk226Unk2Args { val }) => 2,
        Unk3(ScrnUnk226Unk3Args { val1, val2, val3, val4 }) => 3,
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

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
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

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
pub struct TimerArgs {
    pub duration: Expr,
    pub event: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
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
        Unk239 => TypeOp::Unk239,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
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
    use crate::assert_write_and_read;
    use crate::common::Text;
    use crate::data::Music;
    use crate::event::expr::BinaryOp;
    use crate::event::msg::MsgCommand;

    fn expr() -> Expr {
        Expr::Imm32(123)
    }

    fn ip() -> Ip {
        Ip::Offset(123)
    }

    fn binary_op(lhs: Expr, rhs: Expr) -> Box<BinaryOp> {
        Box::new(BinaryOp { rhs, lhs })
    }

    fn if_args() -> Box<IfArgs> {
        Box::new(IfArgs { condition: expr(), else_target: ip() })
    }

    fn text(string: &str) -> Text {
        Text::encode(string).unwrap()
    }

    #[test]
    fn test_write_and_read_command() {
        assert_write_and_read!(Command::Abort);
        assert_write_and_read!(Command::Return);
        assert_write_and_read!(Command::Goto(ip()));
        assert_write_and_read!(Command::Set(Box::new(SetArgs {
            target: SetExpr::from_var(123),
            value: expr(),
        })));
        assert_write_and_read!(Command::Set(Box::new(SetArgs {
            target: SetExpr::from_var(123),
            value: Expr::AddAssign(binary_op(Expr::from_var(123), expr())),
        })));
        assert_write_and_read!(Command::If(if_args()));
        assert_write_and_read!(Command::Elif(if_args()));
        assert_write_and_read!(Command::EndIf(ip()));
        assert_write_and_read!(Command::Case(if_args()));
        assert_write_and_read!(Command::Expr(if_args()));
        assert_write_and_read!(Command::While(if_args()));
        assert_write_and_read!(Command::Break(ip()));
        assert_write_and_read!(Command::Run(ip()));
        assert_write_and_read!(Command::Lib(123));
        assert_write_and_read!(Command::PushBp);
        assert_write_and_read!(Command::PopBp);
        assert_write_and_read!(Command::SetSp(Box::new(expr())));
        assert_write_and_read!(Command::Anim(Box::new(AnimArgs {
            obj: expr(),
            values: vec![expr(), expr(), expr(), expr(), expr(), expr()],
        })));
        assert_write_and_read!(Command::Anim1(Box::new(AnimArgs {
            obj: expr(),
            values: vec![expr(), expr(), expr(), expr(), expr(), expr()],
        })));
        assert_write_and_read!(Command::Anim2(Box::new(AnimArgs {
            obj: expr(),
            values: vec![expr(), expr(), expr(), expr(), expr(), expr()],
        })));
        assert_write_and_read!(Command::Attach(Box::new(AttachArgs {
            obj: expr(),
            event: expr(),
        })));
        assert_write_and_read!(Command::Born(Box::new(BornArgs {
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
        assert_write_and_read!(Command::Call(Box::new(CallArgs {
            obj: expr(),
            args: vec![expr(), expr(), expr()],
        })));
        assert_write_and_read!(Command::Camera(Box::new(CameraType::Anim(CameraAnimArgs {
            val1: expr(),
            val2: expr(),
            val3: expr(),
        }))));
        assert_write_and_read!(Command::Check(Box::new(CheckType::Scale(CheckScaleArgs {
            obj: expr(),
        }))));
        assert_write_and_read!(Command::Color(Box::new(ColorArgs {
            obj: expr(),
            ty: ColorType::Modulate,
            val1: expr(),
            val2: expr(),
            val3: expr(),
            val4: expr(),
        })));
        assert_write_and_read!(Command::Detach(Box::new(expr())));
        assert_write_and_read!(Command::Dir(Box::new(DirArgs { obj: expr(), val: expr() })));
        assert_write_and_read!(Command::MDir(Box::new(MDirArgs {
            obj: expr(),
            ty: MDirType::Dir(MDirDirArgs { val: expr() }),
            val1: expr(),
            val2: expr(),
        })));
        assert_write_and_read!(Command::Disp(Box::new(DispArgs { obj: expr(), disp: expr() })));
        assert_write_and_read!(Command::Kill(Box::new(expr())));
        assert_write_and_read!(Command::Light(Box::new(LightArgs {
            val: expr(),
            ty: LightType::Pos(LightPosArgs { x: expr(), y: expr(), z: expr() }),
        })));
        assert_write_and_read!(Command::Menu(Box::new(MenuType::Item)));
        assert_write_and_read!(Command::Move(Box::new(MoveArgs {
            obj: expr(),
            val1: expr(),
            val2: expr(),
            val3: expr(),
            val4: expr(),
        })));
        assert_write_and_read!(Command::MoveTo(Box::new(MoveToArgs {
            obj: expr(),
            val1: expr(),
            val2: expr(),
            val3: expr(),
            val4: expr(),
            val5: expr(),
            val6: expr(),
        })));
        assert_write_and_read!(Command::Msg(Box::new(MsgArgs::from(text("bunger")))));
        assert_write_and_read!(Command::Pos(Box::new(PosArgs {
            obj: expr(),
            x: expr(),
            y: expr(),
            z: expr(),
        })));
        assert_write_and_read!(Command::PrintF(Box::new(PrintFArgs(text("bunger")))));
        assert_write_and_read!(Command::PrintF(Box::new(PrintFArgs(text("スプラトゥーン")))));
        assert_write_and_read!(Command::Ptcl(Box::new(PtclArgs {
            val: expr(),
            ty: PtclType::Lead(PtclLeadArgs { obj: expr(), args: vec![expr(), expr(), expr()] }),
        })));
        assert_write_and_read!(Command::Read(Box::new(ReadType::Anim(ReadAnimArgs {
            obj: expr(),
            path: expr(),
        }))));
        assert_write_and_read!(Command::Scale(Box::new(ScaleArgs {
            obj: expr(),
            x: expr(),
            y: expr(),
            z: expr(),
        })));
        assert_write_and_read!(Command::MScale(Box::new(MScaleArgs {
            obj: expr(),
            x: expr(),
            y: expr(),
            z: expr(),
            val: expr(),
        })));
        assert_write_and_read!(Command::Scrn(Box::new(ScrnType::Unk234(ScrnUnk234Args {
            val1: expr(),
            val2: expr(),
            val3: expr(),
            val4: expr(),
            val5: expr(),
        }))));
        assert_write_and_read!(Command::Select(Box::new(MsgArgs::from(vec![
            MsgCommand::Text(text("circle")),
            MsgCommand::Newline,
            MsgCommand::Text(text("middle")),
            MsgCommand::Newline,
            MsgCommand::Text(text("radar")),
        ]))));
        assert_write_and_read!(Command::Sfx(Box::new(SfxArgs {
            sfx: SoundExpr::Music(Music::BgmNight),
            ty: SfxType::Stop
        })));
        assert_write_and_read!(Command::Timer(Box::new(TimerArgs {
            duration: expr(),
            event: expr(),
        })));
        assert_write_and_read!(Command::Wait(Box::new(CheckType::Scale(CheckScaleArgs {
            obj: expr(),
        }))));
        assert_write_and_read!(Command::Warp(Box::new(WarpArgs { stage: expr(), val: expr() })));
        assert_write_and_read!(Command::Win(Box::new(WinType::Unk209)));
        assert_write_and_read!(Command::Movie(Box::new(MovieArgs {
            path: expr(),
            val1: expr(),
            val2: expr(),
            val3: expr(),
            val4: expr(),
            val5: expr(),
        })));
    }
}
