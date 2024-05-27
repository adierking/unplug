use crate::opcodes::{AsmMsgOp, NamedOpcode};
use unplug::event::opcodes::{Atom, CmdOp, ExprOp};

/// Describes the type and purpose of an argument or group of arguments.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ArgSignature {
    /// An integer constant.
    Integer,
    /// A string constant.
    String,
    /// Arguments are zero or more message commands.
    Message,
    /// A script pointer constant.
    Pointer,
    /// A color value constant.
    Color,
    /// A sound ID constant.
    Sound,
    /// An expression.
    Expr,
    /// An expression referring to the left-hand side of an assignment.
    SetExpr,
    /// An expression which updates a value in-place.
    UpdateExpr,
    /// An object ID expression.
    ObjectExpr,
    /// An item ID expression.
    ItemExpr,
    /// An attachment ID expression.
    AtcExpr,
    /// A sound ID expression.
    SoundExpr,
    /// A subroutine pointer expression.
    EventExpr,
    /// A string pointer expression.
    StringExpr,
    /// An array pointer expression.
    ArrayExpr,
    /// Zero or more expressions.
    Variadic,
    /// A specific literal integer value.
    LitInteger(i16),
    /// A specific literal atom value.
    LitType(Atom),
}

/// Specifies a valid permutation of arguments for an opcode.
#[derive(Debug)]
pub struct Signature<T: NamedOpcode> {
    pub opcode: T,
    pub args: &'static [ArgSignature],
}

/// Holds all the valid signatures for a type of opcode.
/// Built using the `signature!` macro.
pub struct SignatureSet<T: NamedOpcode + 'static> {
    sigs: &'static [Signature<T>],
}

impl<T: NamedOpcode + 'static> SignatureSet<T> {
    /// Returns a slice over the signatures in order by opcode value.
    pub fn as_slice(&self) -> &'static [Signature<T>] {
        self.sigs
    }

    /// Returns an iterator over the signatures in order by opcode value.
    pub fn iter(&self) -> impl Iterator<Item = &'static Signature<T>> {
        self.sigs.iter()
    }

    /// Returns the number of signatures in the set.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.sigs.len()
    }

    /// Searches for the signatures pertaining to an opcode and extracts a slice of them if found.
    pub fn find_opcode(&self, opcode: T) -> Option<&'static [Signature<T>]> {
        if opcode == T::default() {
            return None;
        }
        if let Ok(index) = self.sigs.binary_search_by_key(&opcode, |s| s.opcode) {
            let eq = |s: &&Signature<T>| s.opcode == opcode;
            let before = self.sigs[..index].iter().rev().take_while(eq).count();
            let after = self.sigs[index..].iter().take_while(eq).count();
            assert!(after > 0, "signature set is not sorted by opcode");
            Some(&self.sigs[(index - before)..(index + after)])
        } else {
            None
        }
    }
}

impl<T: NamedOpcode + 'static> IntoIterator for SignatureSet<T> {
    type Item = &'static Signature<T>;
    type IntoIter = std::slice::Iter<'static, Signature<T>>;
    fn into_iter(self) -> Self::IntoIter {
        self.sigs.iter()
    }
}

/// Declares signature sets for multiple opcode types.
macro_rules! signatures {
    {
        $(
            $name:ident < $type:ident > {
                $( $cmd:ident ( $( $arg:ident $( ( $($subarg:tt)+ ) )? ),* ) ),*
                $(,)*
            }
        )*
    } => {
        $(
            pub static $name: SignatureSet<$type> = SignatureSet {
                sigs: &[
                    $(
                        Signature {
                            opcode: $type::$cmd,
                            args: &[$(signatures!(@arg $arg $( ( $($subarg)+ ) )? ) ),*],
                        }
                    ),*
                ],
            };
        )*
    };
    (@arg Lit($value:literal)) => {
        ArgSignature::LitInteger($value)
    };
    (@arg Lit($name:ident)) => {
        ArgSignature::LitType(Atom::$name)
    };
    (@arg $name:ident) => {
        ArgSignature::$name
    };
}

// IMPORTANT: Binary search is used to find opcodes, so they must be in the same order they're
// declared in their original enums. The tests should catch the mistake if not.
#[rustfmt::skip]
signatures! {
    CMD_SIGNATURES<CmdOp> {
        Abort(),
        Return(),
        Goto(Pointer),
        Set(UpdateExpr),
        Set(SetExpr, Expr),
        If(Expr, Pointer),
        Elif(Expr, Pointer),
        EndIf(Pointer),
        Case(Expr, Pointer),
        Expr(Expr, Pointer),
        While(Expr, Pointer),
        Break(Pointer),
        Run(Pointer),
        Lib(Integer),
        PushBp(),
        PopBp(),
        SetSp(Expr),
        Anim(ObjectExpr, Variadic),
        Anim1(ObjectExpr, Variadic),
        Anim2(ObjectExpr, Variadic),
        Attach(ObjectExpr, EventExpr),
        Born(Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr, EventExpr),
        Call(ObjectExpr, Variadic),
        Camera(Lit(Anim), Expr, Expr, Expr),
        Camera(Lit(Pos), Expr, Expr, Expr, Expr, Expr),
        Camera(Lit(Obj), Expr, Expr, Expr),
        Camera(Lit(Unk209), Expr, Expr),
        Camera(Lit(Unk211), Expr, Expr, Expr, Expr),
        Camera(Lit(Lead), Expr),
        Camera(Lit(Unk227), Expr, Expr, Expr, Expr, Expr),
        Camera(Lit(Distance), Expr, Expr, Expr),
        Camera(Lit(Unk229), Expr, Expr, Expr),
        Camera(Lit(Unk230)),
        Camera(Lit(Unk232), Lit(-2)),
        Camera(Lit(Unk232), Lit(-1)),
        Camera(Lit(Unk232), Lit(0)),
        Camera(Lit(Unk232), Lit(1)),
        Camera(Lit(Unk232), Lit(2), Expr),
        Camera(Lit(Unk232), Lit(3), Expr),
        Camera(Lit(Unk232), Lit(4), Expr),
        Camera(Lit(Unk236), Expr),
        Camera(Lit(Unk237), Expr),
        Camera(Lit(Unk238), Expr),
        Camera(Lit(Unk240), Expr, Expr, Expr, Expr),
        Camera(Lit(Unk243), Expr, Expr, Expr, Expr),
        Camera(Lit(Unk251), Expr, Expr, Expr, Expr),
        Camera(Lit(Unk252), Expr, Expr, Expr, Expr),
        Check(Lit(Time), Expr),
        Check(Lit(Fade)),
        Check(Lit(Wipe)),
        Check(Lit(Unk203)),
        Check(Lit(Anim), ObjectExpr, Expr),
        Check(Lit(Dir), ObjectExpr),
        Check(Lit(Move), ObjectExpr),
        Check(Lit(Color), ObjectExpr),
        Check(Lit(Sfx), SoundExpr),
        Check(Lit(Real), Expr),
        Check(Lit(Cam)),
        Check(Lit(Read), ObjectExpr),
        Check(Lit(ZBlur)),
        Check(Lit(Letterbox)),
        Check(Lit(Shake)),
        Check(Lit(Mono)),
        Check(Lit(Scale), ObjectExpr),
        Check(Lit(Cue)),
        Check(Lit(Unk246), Expr),
        Color(ObjectExpr, Lit(Modulate), Expr, Expr, Expr, Expr),
        Color(ObjectExpr, Lit(Blend), Expr, Expr, Expr, Expr),
        Detach(ObjectExpr),
        Dir(ObjectExpr, Expr),
        MDir(ObjectExpr, Lit(Dir), Expr, Expr, Expr),
        MDir(ObjectExpr, Lit(Pos), Expr, Expr, Expr, Expr),
        MDir(ObjectExpr, Lit(Obj), Expr, Expr, Expr),
        MDir(ObjectExpr, Lit(Cam), Expr, Expr),
        Disp(ObjectExpr, Expr),
        Kill(Expr),
        Light(Expr, Lit(Pos), Expr, Expr, Expr),
        Light(Expr, Lit(Color), Expr, Expr, Expr),
        Light(Expr, Lit(Unk227), Expr, Expr, Expr),
        Menu(Lit(0)),
        Menu(Lit(1)),
        Menu(Lit(2)),
        Menu(Lit(3)),
        Menu(Lit(4)),
        Menu(Lit(5)),
        Menu(Lit(6)),
        Menu(Lit(7)),
        Menu(Lit(1000), Expr),
        Menu(Lit(1001), Expr, Expr),
        Move(ObjectExpr, Expr, Expr, Expr, Expr),
        MoveTo(ObjectExpr, Expr, Expr, Expr, Expr, Expr, Expr),
        Msg(Message),
        Pos(ObjectExpr, Expr, Expr, Expr),
        PrintF(String),
        Ptcl(Expr, Lit(Pos), Expr, Expr, Expr, Expr, Expr, Expr, Expr),
        Ptcl(Expr, Lit(Obj), ObjectExpr, Expr, Expr, Expr, Expr, Expr, Expr, Expr),
        Ptcl(Expr, Lit(Unk210)),
        Ptcl(Expr, Lit(Lead), ObjectExpr, Variadic),
        Read(Lit(Anim), ObjectExpr, StringExpr),
        Read(Lit(Sfx), ObjectExpr, StringExpr),
        Scale(ObjectExpr, Expr, Expr, Expr),
        MScale(ObjectExpr, Expr, Expr, Expr, Expr),
        Scrn(Lit(Fade), Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr),
        Scrn(Lit(Wipe), Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr),
        Scrn(Lit(Hud), Lit(0), Expr),
        Scrn(Lit(Hud), Lit(1), Expr),
        Scrn(Lit(Hud), Lit(2), Expr),
        Scrn(Lit(Hud), Lit(3), Expr, Expr, Expr, Expr),
        Scrn(Lit(Hud), Lit(4), Lit(-4)),
        Scrn(Lit(Hud), Lit(4), Lit(-3), Expr),
        Scrn(Lit(Hud), Lit(4), Lit(-2)),
        Scrn(Lit(Hud), Lit(4), Lit(-1)),
        Scrn(Lit(Hud), Lit(4), Lit(0)),
        Scrn(Lit(Hud), Lit(4), Lit(1)),
        Scrn(Lit(Hud), Lit(4), Lit(2)),
        Scrn(Lit(Hud), Lit(4), Lit(3)),
        Scrn(Lit(ZBlur), Expr, Expr, Expr, Expr, Expr),
        Scrn(Lit(Letterbox), Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr),
        Scrn(Lit(Shake), Expr, Expr, Expr, Expr, Expr, Expr, Expr),
        Scrn(Lit(Mono), Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr, Expr),
        Select(Message),
        Sfx(SoundExpr, Lit(0)),
        Sfx(SoundExpr, Lit(1)),
        Sfx(SoundExpr, Lit(2), Expr),
        Sfx(SoundExpr, Lit(3), Expr),
        Sfx(SoundExpr, Lit(4), Expr, Expr),
        Sfx(SoundExpr, Lit(5)),
        Sfx(SoundExpr, Lit(6)),
        Sfx(SoundExpr, Lit(Cue)),
        Timer(Expr, EventExpr),
        Wait(Lit(Time), Expr),
        Wait(Lit(Fade)),
        Wait(Lit(Wipe)),
        Wait(Lit(Unk203)),
        Wait(Lit(Anim), ObjectExpr, Expr),
        Wait(Lit(Dir), ObjectExpr),
        Wait(Lit(Move), ObjectExpr),
        Wait(Lit(Color), ObjectExpr),
        Wait(Lit(Sfx), SoundExpr),
        Wait(Lit(Real), Expr),
        Wait(Lit(Cam)),
        Wait(Lit(Read), ObjectExpr),
        Wait(Lit(ZBlur)),
        Wait(Lit(Letterbox)),
        Wait(Lit(Shake)),
        Wait(Lit(Mono)),
        Wait(Lit(Scale), ObjectExpr),
        Wait(Lit(Cue)),
        Wait(Lit(Unk246), Expr),
        Warp(Expr, Expr),
        Win(Lit(Pos), Expr, Expr),
        Win(Lit(Obj), ObjectExpr, Expr, Expr, Expr),
        Win(Lit(Unk209)),
        Win(Lit(Color), Expr, Expr, Expr, Expr),
        Win(Lit(Letterbox)),
        Movie(StringExpr, Expr, Expr, Expr, Expr, Expr),
    }

    EXPR_SIGNATURES<ExprOp> {
        Equal(Expr, Expr),
        NotEqual(Expr, Expr),
        Less(Expr, Expr),
        LessEqual(Expr, Expr),
        Greater(Expr, Expr),
        GreaterEqual(Expr, Expr),
        Not(Expr),
        Add(Expr, Expr),
        Subtract(Expr, Expr),
        Multiply(Expr, Expr),
        Divide(Expr, Expr),
        Modulo(Expr, Expr),
        BitAnd(Expr, Expr),
        BitOr(Expr, Expr),
        BitXor(Expr, Expr),
        AddAssign(Expr, Expr),
        SubtractAssign(Expr, Expr),
        MultiplyAssign(Expr, Expr),
        DivideAssign(Expr, Expr),
        ModuloAssign(Expr, Expr),
        BitAndAssign(Expr, Expr),
        BitOrAssign(Expr, Expr),
        BitXorAssign(Expr, Expr),
        Imm16(Integer),
        Imm32(Integer),
        AddressOf(Pointer),
        Stack(Integer),
        ParentStack(Integer),
        Flag(Expr),
        Variable(Expr),
        Result1(),
        Result2(),
        Pad(Expr),
        Battery(Expr),
        Money(),
        Item(ItemExpr),
        Atc(Expr),
        Rank(),
        Exp(),
        Level(),
        Hold(),
        Map(Expr),
        ActorName(ObjectExpr),
        ItemName(ItemExpr),
        Time(Expr),
        CurrentSuit(),
        Scrap(),
        CurrentAtc(),
        Use(),
        Hit(),
        StickerName(Expr),
        Obj(Lit(Anim), ObjectExpr),
        Obj(Lit(Dir), ObjectExpr),
        Obj(Lit(PosX), ObjectExpr),
        Obj(Lit(PosY), ObjectExpr),
        Obj(Lit(PosZ), ObjectExpr),
        Obj(Lit(BoneX), ArrayExpr),
        Obj(Lit(BoneY), ArrayExpr),
        Obj(Lit(BoneZ), ArrayExpr),
        Obj(Lit(DirTo), ArrayExpr),
        Obj(Lit(Distance), ArrayExpr),
        Obj(Lit(Unk235), ObjectExpr),
        Obj(Lit(Unk247), ObjectExpr),
        Obj(Lit(Unk248), ObjectExpr),
        Obj(Lit(Unk249), ArrayExpr),
        Obj(Lit(Unk250), ArrayExpr),
        Random(Expr),
        Sin(Expr),
        Cos(Expr),
        ArrayElement(Expr, Expr, ArrayExpr),
    }

    MSG_SIGNATURES<AsmMsgOp> {
        Speed(Integer),
        Wait(Integer),
        Anim(Integer, Integer, Integer),
        Sfx(Sound, Lit(-1)),
        Sfx(Sound, Lit(0)),
        Sfx(Sound, Lit(1)),
        Sfx(Sound, Lit(2), Integer),
        Sfx(Sound, Lit(3), Integer),
        Sfx(Sound, Lit(4), Integer, Integer),
        Sfx(Sound, Lit(5)),
        Sfx(Sound, Lit(6)),
        Voice(Integer),
        Default(Integer, Integer),
        Format(String),
        Size(Integer),
        Color(Integer),
        Rgba(Color),
        Proportional(Integer),
        Icon(Integer),
        Shake(Integer, Integer, Integer),
        Center(Integer),
        Rotate(Integer),
        Scale(Integer, Integer),
        NumInput(Integer, Integer, Integer),
        Question(Integer, Integer),
        Stay(),
        Text(String),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_traits::NumCast;
    use std::cmp::Ordering;
    use std::fmt::Display;
    use std::process::ExitCode;
    use unplug::common::Text;
    use unplug::event::msg::MsgArgs;
    use unplug::event::opcodes::{ExprOp, Ggte, MsgOp, OpcodeMap};
    use unplug::event::serialize::{
        DeserializeEvent, Error as SerError, EventDeserializer, Result as SerResult,
    };
    use unplug::event::{Command, Expr, Pointer};

    // Number of arguments to synthesize for variadic commands
    const NUM_VARIADICS: i16 = 10;

    /// Values used internally by SignatureDeserializer.
    #[derive(Debug, Copy, Clone)]
    enum Value {
        Integer(i16),
        Command(CmdOp),
        Expr(ExprOp),
        Message(MsgOp),
        String,
        Pointer,
    }

    /// Trait for opcodes that can be converted to SignatureDeserializer values.
    trait SignatureOpcode: NamedOpcode {
        type Deserialized: DeserializeEvent;
        fn into_value(self) -> Value;
    }

    impl SignatureOpcode for CmdOp {
        type Deserialized = Command;
        fn into_value(self) -> Value {
            Value::Command(self)
        }
    }

    impl SignatureOpcode for ExprOp {
        type Deserialized = Expr;
        fn into_value(self) -> Value {
            Value::Expr(self)
        }
    }

    impl SignatureOpcode for AsmMsgOp {
        type Deserialized = MsgArgs;
        fn into_value(self) -> Value {
            match self {
                Self::Text => Value::String,
                _ => Value::Message(self.try_into().unwrap()),
            }
        }
    }

    /// A deserializer that converts a signature into values for the purpose of checking that a
    /// signature is correct.
    struct SignatureDeserializer {
        values: Vec<Value>,
        index: usize,
    }

    impl SignatureDeserializer {
        fn new<T: SignatureOpcode>(sig: &Signature<T>) -> Self {
            Self { values: Self::build_values(sig.opcode.into_value(), sig.args), index: 0 }
        }

        fn remaining(&self) -> &[Value] {
            &self.values[self.index..]
        }

        fn next(&mut self) -> Option<Value> {
            if self.index < self.values.len() {
                let index = self.index;
                self.index += 1;
                Some(self.values[index])
            } else {
                None
            }
        }

        fn next_integer<T: Default + NumCast>(&mut self) -> SerResult<T> {
            match self.next() {
                Some(Value::Integer(i)) => Ok(T::from(i).unwrap()),
                Some(Value::Expr(ExprOp::Imm32)) => {
                    // Convert expressions into raw integers as necessary. Most of the time integer
                    // literals are expressions, but not in messages.
                    self.next_integer()
                }
                _ => Err(SerError::ExpectedInteger),
            }
        }

        fn build_values(opcode: Value, args: &[ArgSignature]) -> Vec<Value> {
            let mut values = vec![opcode];
            for i in 0..args.len() {
                let mut index = i;
                if matches!(opcode, Value::Command(CmdOp::Set)) {
                    index = args.len() - index - 1;
                }
                match args[index] {
                    ArgSignature::Integer | ArgSignature::Color | ArgSignature::Sound => {
                        values.push(Value::Integer(0));
                    }
                    ArgSignature::String => {
                        values.push(Value::String);
                    }
                    ArgSignature::Message => {
                        values.push(Value::Message(MsgOp::End));
                    }
                    ArgSignature::Pointer => {
                        values.push(Value::Pointer);
                    }
                    ArgSignature::Expr | ArgSignature::ObjectExpr | ArgSignature::SoundExpr => {
                        values.push(Value::Expr(ExprOp::Imm32));
                        values.push(Value::Integer(0));
                    }
                    ArgSignature::SetExpr => {
                        values.push(Value::Expr(ExprOp::Variable));
                        values.push(Value::Expr(ExprOp::Imm32));
                        values.push(Value::Integer(0));
                    }
                    ArgSignature::UpdateExpr => {
                        values.push(Value::Expr(ExprOp::AddAssign));
                        values.push(Value::Expr(ExprOp::Imm32));
                        values.push(Value::Integer(0));
                        values.push(Value::Expr(ExprOp::Variable));
                        values.push(Value::Expr(ExprOp::Imm32));
                        values.push(Value::Integer(0));
                    }
                    ArgSignature::EventExpr
                    | ArgSignature::StringExpr
                    | ArgSignature::ArrayExpr
                    | ArgSignature::ItemExpr
                    | ArgSignature::AtcExpr => {
                        values.push(Value::Expr(ExprOp::AddressOf));
                        values.push(Value::Pointer);
                    }
                    ArgSignature::Variadic => {
                        for j in 0..NUM_VARIADICS {
                            values.push(Value::Expr(ExprOp::Imm32));
                            values.push(Value::Integer(j));
                        }
                    }
                    ArgSignature::LitType(t) => {
                        values.push(Value::Expr(ExprOp::Imm32));
                        values.push(Value::Integer(Ggte::value(t).unwrap() as i16));
                    }
                    ArgSignature::LitInteger(i) => {
                        values.push(Value::Expr(ExprOp::Imm32));
                        values.push(Value::Integer(i));
                    }
                }
            }
            if matches!(values[0], Value::Message(MsgOp::Format)) {
                // HACK: format strings have to end with the format character
                values.push(Value::Message(MsgOp::Format));
            }
            values
        }
    }

    impl EventDeserializer for SignatureDeserializer {
        fn deserialize_i8(&mut self) -> SerResult<i8> {
            self.next_integer()
        }

        fn deserialize_u8(&mut self) -> SerResult<u8> {
            self.next_integer()
        }

        fn deserialize_i16(&mut self) -> SerResult<i16> {
            self.next_integer()
        }

        fn deserialize_u16(&mut self) -> SerResult<u16> {
            self.next_integer()
        }

        fn deserialize_i32(&mut self) -> SerResult<i32> {
            self.next_integer()
        }

        fn deserialize_u32(&mut self) -> SerResult<u32> {
            self.next_integer()
        }

        fn deserialize_pointer(&mut self) -> SerResult<Pointer> {
            match self.next() {
                Some(Value::Pointer) => Ok(Pointer::Offset(0)),
                _ => Err(SerError::ExpectedPointer),
            }
        }

        fn deserialize_i8_array(&mut self, _len: usize) -> SerResult<Vec<i8>> {
            unimplemented!()
        }

        fn deserialize_u8_array(&mut self, _len: usize) -> SerResult<Vec<u8>> {
            unimplemented!()
        }

        fn deserialize_i16_array(&mut self, _len: usize) -> SerResult<Vec<i16>> {
            unimplemented!()
        }

        fn deserialize_u16_array(&mut self, _len: usize) -> SerResult<Vec<u16>> {
            unimplemented!()
        }

        fn deserialize_i32_array(&mut self, _len: usize) -> SerResult<Vec<i32>> {
            unimplemented!()
        }

        fn deserialize_u32_array(&mut self, _len: usize) -> SerResult<Vec<u32>> {
            unimplemented!()
        }

        fn deserialize_pointer_array(&mut self, _max_len: usize) -> SerResult<Vec<Pointer>> {
            unimplemented!()
        }

        fn deserialize_atom(&mut self) -> SerResult<Atom> {
            match self.begin_expr()? {
                ExprOp::Imm32 => {
                    let value = self.deserialize_i32()?;
                    Ggte::get(value).map_err(SerError::UnrecognizedAtom)
                }
                _ => Err(SerError::ExpectedAtom),
            }
        }

        fn deserialize_text(&mut self) -> SerResult<Text> {
            match self.next() {
                Some(Value::String) => Ok(Text::new()),
                _ => Err(SerError::ExpectedText),
            }
        }

        fn deserialize_rgba(&mut self) -> SerResult<u32> {
            self.next_integer()
        }

        fn begin_expr(&mut self) -> SerResult<ExprOp> {
            match self.next() {
                Some(Value::Expr(e)) => Ok(e),
                _ => Err(SerError::ExpectedExpr),
            }
        }

        fn end_expr(&mut self) -> SerResult<()> {
            Ok(())
        }

        fn begin_command(&mut self) -> SerResult<CmdOp> {
            match self.next() {
                Some(Value::Command(c)) => Ok(c),
                _ => Err(SerError::ExpectedCommand),
            }
        }

        fn end_command(&mut self) -> SerResult<()> {
            Ok(())
        }

        fn begin_variadic_args(&mut self) -> SerResult<()> {
            Ok(())
        }

        fn have_variadic_arg(&mut self) -> SerResult<bool> {
            Ok(self.index < self.values.len())
        }

        fn end_variadic_args(&mut self) -> SerResult<()> {
            Ok(())
        }

        fn deserialize_msg_char(&mut self) -> SerResult<MsgOp> {
            match self.next() {
                Some(Value::Message(m)) => Ok(m),
                Some(Value::String) => Ok(MsgOp::Char(b'x')),
                None => Ok(MsgOp::End),
                _ => Err(SerError::ExpectedMessage),
            }
        }
    }

    fn do_signature_test<T: SignatureOpcode>(signatures: &SignatureSet<T>) -> ExitCode
    where
        <T::Deserialized as DeserializeEvent>::Error: Display,
    {
        let mut success = 0;
        for (i, sig) in signatures.iter().enumerate() {
            print!("{i:>3}. {:?}{:?}...", sig.opcode, sig.args);
            let mut deserializer = SignatureDeserializer::new(sig);
            match T::Deserialized::deserialize(&mut deserializer) {
                Ok(_) => {
                    let remaining = deserializer.remaining();
                    if remaining.is_empty() {
                        success += 1;
                        println!("OK");
                    } else {
                        println!("ERROR!\n     Unused value(s): {:?}", remaining);
                    }
                }
                Err(e) => println!("ERROR!\n     {:#}", e),
            }
        }
        let total = signatures.len();
        println!("{success}/{total} signatures validated");
        match success.cmp(&total) {
            Ordering::Equal => ExitCode::SUCCESS,
            _ => ExitCode::FAILURE,
        }
    }

    fn do_find_opcode_test<T: SignatureOpcode>(signatures: &SignatureSet<T>) {
        let mut i = 0;
        let all = signatures.as_slice();
        while i < all.len() {
            let opcode = all[i].opcode;
            let mut end = i + 1;
            while end < all.len() && all[end].opcode == opcode {
                end += 1;
            }
            let found = signatures.find_opcode(opcode).map(|s| s.len()).unwrap_or(0);
            assert_eq!(found, end - i, "{:?}", opcode);
            println!("{:?} {}", opcode, found);
            i = end;
        }
    }

    #[test]
    fn test_command_signatures() -> ExitCode {
        do_signature_test(&CMD_SIGNATURES)
    }

    #[test]
    fn test_expr_signatures() -> ExitCode {
        do_signature_test(&EXPR_SIGNATURES)
    }

    #[test]
    fn test_message_signatures() -> ExitCode {
        do_signature_test(&MSG_SIGNATURES)
    }

    #[test]
    fn test_find_opcode_command() {
        do_find_opcode_test(&CMD_SIGNATURES);
    }

    #[test]
    fn test_find_opcode_expr() {
        do_find_opcode_test(&EXPR_SIGNATURES);
    }

    #[test]
    fn test_find_opcode_msg() {
        do_find_opcode_test(&MSG_SIGNATURES);
    }
}
