use super::block::WriteIp;
use super::expr::{self, Expr, SetExpr};
use super::msg::{self, MsgArgs};
use super::opcodes::*;
use super::Ip;
use crate::common::io::write_u8_and;
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
    pub fn is_if(&self) -> bool {
        self.if_args().is_some()
    }

    /// If the command is an `if` statement, retrieve its `IfArgs`.
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
    pub fn is_goto(&self) -> bool {
        self.goto_target().is_some()
    }

    /// If the command always jumps to another offset, retrieve the target.
    pub fn goto_target(&self) -> Option<&Ip> {
        match self {
            Self::Break(x) => Some(x),
            Self::EndIf(x) => Some(x),
            Self::Goto(x) => Some(x),
            _ => None,
        }
    }

    /// If the command always jumps to another offset, retrieve a mutable reference to the target.
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
}

impl<R: Read + Seek + ?Sized> ReadFrom<R> for Command {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let cmd = reader.read_u8()?;
        Ok(match cmd {
            CMD_ABORT => Self::Abort,
            CMD_RETURN => Self::Return,
            CMD_GOTO => Self::Goto(Ip::read_from(reader)?),
            CMD_SET => Self::Set(SetArgs::read_from(reader)?.into()),
            CMD_IF => Self::If(IfArgs::read_from(reader)?.into()),
            CMD_ELIF => Self::Elif(IfArgs::read_from(reader)?.into()),
            CMD_ENDIF => Self::EndIf(Ip::read_from(reader)?),
            CMD_CASE => Self::Case(IfArgs::read_from(reader)?.into()),
            CMD_EXPR => Self::Expr(IfArgs::read_from(reader)?.into()),
            CMD_WHILE => Self::While(IfArgs::read_from(reader)?.into()),
            CMD_BREAK => Self::Break(Ip::read_from(reader)?),
            CMD_RUN => Self::Run(Ip::read_from(reader)?),
            CMD_LIB => Self::Lib(reader.read_i16::<LE>()?),
            CMD_PUSHBP => Self::PushBp,
            CMD_POPBP => Self::PopBp,
            CMD_SETSP => Self::SetSp(Expr::read_from(reader)?.into()),
            CMD_ANIM => Self::Anim(AnimArgs::read_from(reader)?.into()),
            CMD_ANIM1 => Self::Anim1(AnimArgs::read_from(reader)?.into()),
            CMD_ANIM2 => Self::Anim2(AnimArgs::read_from(reader)?.into()),
            CMD_ATTACH => Self::Attach(AttachArgs::read_from(reader)?.into()),
            CMD_BORN => Self::Born(BornArgs::read_from(reader)?.into()),
            CMD_CALL => Self::Call(CallArgs::read_from(reader)?.into()),
            CMD_CAMERA => Self::Camera(CameraType::read_from(reader)?.into()),
            CMD_CHECK => Self::Check(CheckType::read_from(reader)?.into()),
            CMD_COLOR => Self::Color(ColorArgs::read_from(reader)?.into()),
            CMD_DETACH => Self::Detach(Expr::read_from(reader)?.into()),
            CMD_DIR => Self::Dir(DirArgs::read_from(reader)?.into()),
            CMD_MDIR => Self::MDir(MDirArgs::read_from(reader)?.into()),
            CMD_DISP => Self::Disp(DispArgs::read_from(reader)?.into()),
            CMD_KILL => Self::Kill(Expr::read_from(reader)?.into()),
            CMD_LIGHT => Self::Light(LightArgs::read_from(reader)?.into()),
            CMD_MENU => Self::Menu(MenuType::read_from(reader)?.into()),
            CMD_MOVE => Self::Move(MoveArgs::read_from(reader)?.into()),
            CMD_MOVETO => Self::MoveTo(MoveToArgs::read_from(reader)?.into()),
            CMD_MSG => Self::Msg(MsgArgs::read_from(reader)?.into()),
            CMD_POS => Self::Pos(PosArgs::read_from(reader)?.into()),
            CMD_PRINTF => Self::PrintF(PrintFArgs::read_from(reader)?.into()),
            CMD_PTCL => Self::Ptcl(PtclArgs::read_from(reader)?.into()),
            CMD_READ => Self::Read(ReadType::read_from(reader)?.into()),
            CMD_SCALE => Self::Scale(ScaleArgs::read_from(reader)?.into()),
            CMD_MSCALE => Self::MScale(MScaleArgs::read_from(reader)?.into()),
            CMD_SCRN => Self::Scrn(ScrnType::read_from(reader)?.into()),
            CMD_SELECT => Self::Select(MsgArgs::read_from(reader)?.into()),
            CMD_SFX => Self::Sfx(SfxArgs::read_from(reader)?.into()),
            CMD_TIMER => Self::Timer(TimerArgs::read_from(reader)?.into()),
            CMD_WAIT => Self::Wait(CheckType::read_from(reader)?.into()),
            CMD_WARP => Self::Warp(WarpArgs::read_from(reader)?.into()),
            CMD_WIN => Self::Win(WinType::read_from(reader)?.into()),
            CMD_MOVIE => Self::Movie(MovieArgs::read_from(reader)?.into()),
            _ => return Err(Error::UnrecognizedOp(cmd)),
        })
    }
}

impl<W: Write + WriteIp + Seek + ?Sized> WriteTo<W> for Command {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        match self {
            Self::Abort => writer.write_u8(CMD_ABORT)?,
            Self::Return => writer.write_u8(CMD_RETURN)?,
            Self::Goto(ip) => write_u8_and(writer, CMD_GOTO, ip)?,
            Self::Set(arg) => write_u8_and(writer, CMD_SET, &**arg)?,
            Self::If(arg) => write_u8_and(writer, CMD_IF, &**arg)?,
            Self::Elif(arg) => write_u8_and(writer, CMD_ELIF, &**arg)?,
            Self::EndIf(ip) => write_u8_and(writer, CMD_ENDIF, ip)?,
            Self::Case(arg) => write_u8_and(writer, CMD_CASE, &**arg)?,
            Self::Expr(arg) => write_u8_and(writer, CMD_EXPR, &**arg)?,
            Self::While(arg) => write_u8_and(writer, CMD_WHILE, &**arg)?,
            Self::Break(ip) => write_u8_and(writer, CMD_BREAK, ip)?,
            Self::Run(ip) => write_u8_and(writer, CMD_RUN, ip)?,
            Self::Lib(index) => {
                writer.write_u8(CMD_LIB)?;
                writer.write_i16::<LE>(*index)?;
            }
            Self::PushBp => writer.write_u8(CMD_PUSHBP)?,
            Self::PopBp => writer.write_u8(CMD_POPBP)?,
            Self::SetSp(arg) => write_u8_and(writer, CMD_SETSP, &**arg)?,
            Self::Anim(arg) => write_u8_and(writer, CMD_ANIM, &**arg)?,
            Self::Anim1(arg) => write_u8_and(writer, CMD_ANIM1, &**arg)?,
            Self::Anim2(arg) => write_u8_and(writer, CMD_ANIM2, &**arg)?,
            Self::Attach(arg) => write_u8_and(writer, CMD_ATTACH, &**arg)?,
            Self::Born(arg) => write_u8_and(writer, CMD_BORN, &**arg)?,
            Self::Call(arg) => write_u8_and(writer, CMD_CALL, &**arg)?,
            Self::Camera(arg) => write_u8_and(writer, CMD_CAMERA, &**arg)?,
            Self::Check(arg) => write_u8_and(writer, CMD_CHECK, &**arg)?,
            Self::Color(arg) => write_u8_and(writer, CMD_COLOR, &**arg)?,
            Self::Detach(arg) => write_u8_and(writer, CMD_DETACH, &**arg)?,
            Self::Dir(arg) => write_u8_and(writer, CMD_DIR, &**arg)?,
            Self::MDir(arg) => write_u8_and(writer, CMD_MDIR, &**arg)?,
            Self::Disp(arg) => write_u8_and(writer, CMD_DISP, &**arg)?,
            Self::Kill(arg) => write_u8_and(writer, CMD_KILL, &**arg)?,
            Self::Light(arg) => write_u8_and(writer, CMD_LIGHT, &**arg)?,
            Self::Menu(arg) => write_u8_and(writer, CMD_MENU, &**arg)?,
            Self::Move(arg) => write_u8_and(writer, CMD_MOVE, &**arg)?,
            Self::MoveTo(arg) => write_u8_and(writer, CMD_MOVETO, &**arg)?,
            Self::Msg(arg) => write_u8_and(writer, CMD_MSG, &**arg)?,
            Self::Pos(arg) => write_u8_and(writer, CMD_POS, &**arg)?,
            Self::PrintF(arg) => write_u8_and(writer, CMD_PRINTF, &**arg)?,
            Self::Ptcl(arg) => write_u8_and(writer, CMD_PTCL, &**arg)?,
            Self::Read(arg) => write_u8_and(writer, CMD_READ, &**arg)?,
            Self::Scale(arg) => write_u8_and(writer, CMD_SCALE, &**arg)?,
            Self::MScale(arg) => write_u8_and(writer, CMD_MSCALE, &**arg)?,
            Self::Scrn(arg) => write_u8_and(writer, CMD_SCRN, &**arg)?,
            Self::Select(arg) => write_u8_and(writer, CMD_SELECT, &**arg)?,
            Self::Sfx(arg) => write_u8_and(writer, CMD_SFX, &**arg)?,
            Self::Timer(arg) => write_u8_and(writer, CMD_TIMER, &**arg)?,
            Self::Wait(arg) => write_u8_and(writer, CMD_WAIT, &**arg)?,
            Self::Warp(arg) => write_u8_and(writer, CMD_WARP, &**arg)?,
            Self::Win(arg) => write_u8_and(writer, CMD_WIN, &**arg)?,
            Self::Movie(arg) => write_u8_and(writer, CMD_MOVIE, &**arg)?,
        }
        Ok(())
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
        Anim(CameraAnimArgs { val1, val2, val3 }) => TYPE_ANIM,
        Pos(CameraPosArgs { val1, val2, val3, val4, val5 }) => TYPE_POS,
        Obj(CameraObjArgs { val1, val2, val3 }) => TYPE_OBJ,
        Unk209(CameraUnk209Args { val1, val2 }) => TYPE_UNK_209,
        Unk211(CameraUnk211Args { val1, val2, val3, val4 }) => TYPE_UNK_211,
        Lead(CameraLeadArgs { val }) => TYPE_LEAD,
        Unk227(CameraUnk227Args { val1, val2, val3, val4, val5 }) => TYPE_UNK_227,
        Unk228(CameraDistanceArgs { val1, val2, val3 }) => TYPE_DISTANCE,
        Unk229(CameraUnk229Args { val1, val2, val3 }) => TYPE_UNK_229,
        Unk230 => TYPE_UNK_230,
        Unk232(CameraUnk232Type) => TYPE_UNK_232,
        Unk236(CameraUnk236Args { val }) => TYPE_UNK_236,
        Unk237(CameraUnk237Args { val }) => TYPE_UNK_237,
        Unk238(CameraUnk238Args { val }) => TYPE_UNK_238,
        Unk240(CameraUnk240Args { val1, val2, val3, val4 }) => TYPE_UNK_240,
        Unk243(CameraUnk243Args { val1, val2, val3, val4 }) => TYPE_UNK_243,
        Unk251(CameraUnk251Args { val1, val2, val3, val4 }) => TYPE_UNK_251,
        Unk252(CameraUnk252Args { val1, val2, val3, val4 }) => TYPE_UNK_252,
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
        Time(CheckTimeArgs { duration }) => TYPE_TIME,
        Unk201 => TYPE_UNK_201,
        Wipe => TYPE_WIPE,
        Unk203 => TYPE_UNK_203,
        Anim(CheckAnimArgs { obj, val }) => TYPE_ANIM,
        Dir(CheckDirArgs { obj }) => TYPE_DIR,
        Move(CheckMoveArgs { obj }) => TYPE_MOVE,
        Color(CheckColorArgs { obj }) => TYPE_COLOR,
        Sfx(CheckSfxArgs { val }) => TYPE_SFX,
        Real(CheckRealArgs { val }) => TYPE_REAL,
        Cam => TYPE_CAM,
        Read(CheckReadArgs { obj }) => TYPE_READ,
        Unk234 => TYPE_UNK_234,
        Unk239 => TYPE_UNK_239,
        Unk241 => TYPE_UNK_241,
        Unk242 => TYPE_UNK_242,
        Scale(CheckScaleArgs { obj }) => TYPE_SCALE,
        Marker => TYPE_MARKER,
        Unk246(CheckUnk246Args { val }) => TYPE_UNK_246,
    }
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
        Modulate => TYPE_MODULATE,
        Blend => TYPE_BLEND,
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
        Dir(MDirDirArgs { val }) => TYPE_DIR,
        Pos(MDirPosArgs { val1, val2 }) => TYPE_POS,
        Obj(MDirObjArgs { val }) => TYPE_OBJ,
        Cam => TYPE_CAM,
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
        Pos(LightPosArgs { x, y, z }) => TYPE_POS,
        Color(LightColorArgs { r, g, b }) => TYPE_COLOR,
        Unk227(LightUnk227Args { val1, val2, val3 }) => TYPE_UNK_227,
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
        Pos(PtclPosArgs { val1, val2, val3, val4, val5, val6, val7 }) => TYPE_POS,
        Obj(PtclObjArgs { obj, val1, val2, val3, val4, val5, val6, val7 }) => TYPE_OBJ,
        Unk210 => TYPE_UNK_210,
        Lead(PtclLeadArgs) => TYPE_LEAD,
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
        Anim(ReadAnimArgs { obj, path }) => TYPE_ANIM,
        Sfx(ReadSfxArgs { obj, path }) => TYPE_SFX,
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
        }) => TYPE_UNK_201,
        Wipe(ScrnWipeArgs {
            val1, val2, val3, val4, val5, val6, val7, val8, val9, val10,
            val11, val12, val13, val14, val15, val16, val17
        }) => TYPE_WIPE,
        Unk226(ScrnUnk226Type) => TYPE_UNK_226,
        Unk234(ScrnUnk234Args { val1, val2, val3, val4, val5 }) => TYPE_UNK_234,
        Unk239(ScrnUnk239Args {
            val1, val2, val3, val4, val5, val6, val7, val8, val9, val10
        }) => TYPE_UNK_239,
        Unk241(ScrnUnk241Args { val1, val2, val3, val4, val5, val6, val7 }) => TYPE_UNK_241,
        Unk242(ScrnUnk242Args {
            val1, val2, val3, val4, val5, val6, val7, val8, val9
        }) => TYPE_UNK_242,
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
    pub val: Expr,
    pub ty: SfxType,
}

expr_enum! {
    type Error = Error;
    pub enum SfxType {
        Stop => SFX_STOP,
        Play => SFX_PLAY,
        FadeOut(SfxFadeOutArgs { duration }) => SFX_FADE_OUT,
        FadeIn(SfxFadeInArgs { duration }) => SFX_FADE_IN,
        Fade(SfxFadeArgs { duration, volume }) => SFX_FADE,
        Unk5 => SFX_UNK_5,
        Unk6 => SFX_UNK_6,
        Unk245 => SFX_UNK_245,
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
        Pos(WinPosArgs { val1, val2 }) => TYPE_POS,
        Obj(WinObjArgs { obj, val1, val2, val3 }) => TYPE_OBJ,
        Unk209 => TYPE_UNK_209,
        Color(WinColorArgs { val1, val2, val3, val4 }) => TYPE_COLOR,
        Unk239 => TYPE_UNK_239,
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
        assert_write_and_read!(Command::Sfx(Box::new(SfxArgs { val: expr(), ty: SfxType::Stop })));
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
