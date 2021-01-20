use super::block::BlockInfo;
use super::subroutine::{SubroutineEffects, SubroutineInfoMap};
use super::value::{ArrayKind, Definition, DefinitionMap, Label, Value, ValueKind};
use crate::event::command::*;
use crate::event::expr::{ArrayElementExpr, BinaryOp, Expr, ObjExpr, SetExpr};
use crate::event::msg::{MsgArgs, MsgCommand};
use crate::event::{BlockId, Ip};
use arrayvec::ArrayVec;
use log::warn;
use std::collections::{hash_map, HashMap, HashSet};

/// A value which is live in the middle of a block.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LiveValue {
    /// A single concrete value.
    Value(Value),
    /// Multiple concrete values.
    Union(Vec<Value>),
    /// The value is a reference to an array element.
    ArrayElement(Box<LiveValue>),
    /// The value dereferences (i.e. reads the value at the address contained in) another value.
    Deref(Box<LiveValue>),
    /// The value is something we aren't interested in analyzing (e.g. a constant).
    Other,
}

impl LiveValue {
    #[cfg(test)]
    fn value(&self) -> Value {
        match self {
            Self::Value(value) => *value,
            _ => panic!("Not a singular value"),
        }
    }

    /// Appends the values contained by another value into this value.
    fn append(&mut self, other: LiveValue) {
        let mut values: Vec<Value> = match self {
            LiveValue::Value(value) => vec![*value],
            LiveValue::Union(values) => values.split_off(0),
            LiveValue::ArrayElement(_) | LiveValue::Deref(_) | LiveValue::Other => vec![],
        };
        match other {
            LiveValue::Value(other) => values.push(other),
            LiveValue::Union(other) => values.extend(other),
            LiveValue::ArrayElement(_) | LiveValue::Deref(_) | LiveValue::Other => (),
        }
        *self = values.into();
    }
}

impl From<Value> for LiveValue {
    fn from(value: Value) -> Self {
        Self::Value(value)
    }
}

impl From<Vec<Value>> for LiveValue {
    fn from(values: Vec<Value>) -> Self {
        match values.len() {
            0 => Self::Other,
            1 => Self::Value(values[0]),
            _ => Self::Union(values),
        }
    }
}

/// The state of all live information in the middle of a block.
pub(super) struct LiveState {
    /// The values which are currently live.
    values: HashMap<Label, LiveValue>,
    /// The labels which have been killed so far.
    killed: HashSet<Label>,
    /// The values which have been referenced so far.
    references: Vec<(ValueKind, Value)>,
    /// The value of the stack pointer at each stack frame.
    sp_stack: Vec<u8>,
    /// The current value of the stack pointer.
    sp: u8,
}

impl LiveState {
    /// Constructs an empty `LiveState`.
    pub(super) fn new() -> Self {
        Self {
            values: HashMap::new(),
            killed: HashSet::new(),
            references: vec![],
            sp_stack: vec![],
            sp: 0,
        }
    }

    /// Finalizes the state into a `BlockInfo`. New definitions will be inserted into `defs`.
    pub(super) fn into_block(self, block_id: BlockId, defs: &mut DefinitionMap) -> BlockInfo {
        let mut generated = HashSet::new();
        for (label, value) in self.values {
            match value {
                LiveValue::Value(value) => {
                    let id = defs.insert(Definition { label, origin: Some(block_id), value });
                    generated.insert(id);
                }
                LiveValue::Union(values) => {
                    for value in values {
                        let id = defs.insert(Definition { label, origin: Some(block_id), value });
                        generated.insert(id);
                    }
                }
                LiveValue::ArrayElement(_) | LiveValue::Deref(_) | LiveValue::Other => (),
            }
        }
        BlockInfo {
            id: block_id,
            predecessors: vec![],
            successors: ArrayVec::new(),
            inputs: HashSet::new(),
            outputs: generated.clone(),
            generated,
            killed: self.killed,
            undefined: HashSet::new(),
            references: self.references,
        }
    }

    /// Analyzes a command and updates the state.
    pub(super) fn analyze_command(
        &mut self,
        cmd: &Command,
        subs: &SubroutineInfoMap,
        libs: &[SubroutineEffects],
    ) {
        // TODO: Maybe make a generic interface to iterate over a command's expressions
        match cmd {
            Command::Abort => (),
            Command::Return => (),
            Command::Goto(_) => (),
            Command::Set(arg) => self.analyze_set(&arg.target, &arg.value),
            Command::If(arg)
            | Command::Elif(arg)
            | Command::Case(arg)
            | Command::Expr(arg)
            | Command::While(arg) => self.analyze_if(arg),
            Command::EndIf(_) => (),
            Command::Break(_) => (),
            Command::Run(ip) => self.analyze_run(*ip, subs),
            Command::Lib(index) => self.analyze_lib(*index, libs),
            Command::PushBp => self.analyze_push_bp(),
            Command::PopBp => self.analyze_pop_bp(),
            Command::SetSp(e) => self.analyze_set_sp(e),
            Command::Anim(_) => (),
            Command::Anim1(_) => (),
            Command::Anim2(_) => (),
            Command::Attach(arg) => self.analyze_attach(arg),
            Command::Born(arg) => self.analyze_born(arg),
            Command::Call(arg) => self.analyze_call(arg),
            Command::Camera(_) => (),
            Command::Check(_) => (),
            Command::Color(_) => (),
            Command::Detach(arg) => self.analyze_detach(arg),
            Command::Dir(_) => (),
            Command::MDir(_) => (),
            Command::Disp(_) => (),
            Command::Kill(_) => (),
            Command::Light(_) => (),
            Command::Menu(_) => (),
            Command::Move(_) => (),
            Command::MoveTo(_) => (),
            Command::Msg(arg) => self.analyze_msg(arg),
            Command::Pos(_) => (),
            Command::PrintF(_) => (),
            Command::Ptcl(_) => (),
            Command::Read(arg) => self.analyze_read(arg),
            Command::Scale(_) => (),
            Command::MScale(_) => (),
            Command::Scrn(_) => (),
            Command::Select(_) => (),
            Command::Sfx(_) => (),
            Command::Timer(arg) => self.analyze_timer(arg),
            Command::Wait(_) => (),
            Command::Warp(_) => (),
            Command::Win(_) => (),
            Command::Movie(arg) => self.analyze_movie(arg),
        }
    }

    fn analyze_attach(&mut self, arg: &AttachArgs) {
        self.analyze_expr(&arg.obj);
        self.analyze_reference(ValueKind::Event, &arg.event);
    }

    fn analyze_born(&mut self, arg: &BornArgs) {
        self.analyze_expr(&arg.val1);
        self.analyze_expr(&arg.val2);
        self.analyze_expr(&arg.val3);
        self.analyze_expr(&arg.val4);
        self.analyze_expr(&arg.val5);
        self.analyze_expr(&arg.val6);
        self.analyze_expr(&arg.val7);
        self.analyze_expr(&arg.val8);
        self.analyze_expr(&arg.val9);
        self.analyze_reference(ValueKind::Event, &arg.event);
    }

    fn analyze_call(&mut self, arg: &CallArgs) {
        if let Some(value) = arg.obj.value() {
            // Special cases for system calls
            if value == -200 {
                if arg.args.len() >= 2 {
                    self.analyze_reference(ValueKind::String, &arg.args[0]);
                    self.analyze_reference(ValueKind::Event, &arg.args[1]);
                    return;
                } else {
                    warn!("Not enough arguments for call(-200)");
                }
            }
        } else {
            self.analyze_expr(&arg.obj);
        }
        for arg in &arg.args {
            // Sometimes scripts pass arbitrary data to native functions
            if let Expr::AddressOf(_) = arg {
                self.analyze_reference(ValueKind::Array(ArrayKind::U8), arg);
            } else {
                self.analyze_expr(arg);
            }
        }
        // Assume call() always mutates Result1
        self.set_value(Label::Result1, LiveValue::Other);
    }

    fn analyze_detach(&mut self, arg: &Expr) {
        self.analyze_expr(arg);
    }

    fn analyze_if(&mut self, arg: &IfArgs) {
        self.analyze_expr(&arg.condition);
    }

    fn analyze_lib(&mut self, index: i16, libs: &[SubroutineEffects]) {
        if libs.is_empty() {
            panic!("No library subroutines are configured");
        }
        if index < 0 || (index as usize) >= libs.len() {
            // TODO: this should be an error, not a panic
            panic!("Invalid library index: {}", index);
        }
        self.analyze_sub_call(&libs[index as usize]);
    }

    fn analyze_movie(&mut self, arg: &MovieArgs) {
        self.analyze_reference(ValueKind::String, &arg.path);
        self.analyze_expr(&arg.val1);
        self.analyze_expr(&arg.val2);
        self.analyze_expr(&arg.val3);
        self.analyze_expr(&arg.val4);
        self.analyze_expr(&arg.val5);
    }

    fn analyze_push_bp(&mut self) {
        // Create a new stack frame
        self.sp_stack.push(self.sp);
        self.sp = 0;
    }

    fn analyze_pop_bp(&mut self) {
        if let Some(sp) = self.sp_stack.pop() {
            self.sp = sp;
            let bp = self.sp_stack.len() as i16;
            // Discard any live values that belonged to the stack frame
            self.values.retain(
                |&label, _| {
                    if let Label::Stack(lbp, _) = label {
                        lbp <= bp
                    } else {
                        true
                    }
                },
            );
        }
    }

    fn analyze_msg(&mut self, arg: &MsgArgs) {
        // If the message prompts for user input, it sets Result1
        for command in &arg.commands {
            if let MsgCommand::NumInput(_) | MsgCommand::Question(_) = command {
                self.set_value(Label::Result1, LiveValue::Other);
                return;
            }
        }
    }

    fn analyze_read(&mut self, arg: &ReadType) {
        match arg {
            ReadType::Anim(anim) => {
                self.analyze_expr(&anim.obj);
                self.analyze_reference(ValueKind::String, &anim.path);
            }
            ReadType::Sfx(sfx) => {
                self.analyze_expr(&sfx.obj);
                self.analyze_reference(ValueKind::String, &sfx.path);
            }
        }
    }

    fn analyze_run(&mut self, ip: Ip, subs: &SubroutineInfoMap) {
        let block_id = ip.block().expect("Unresolved subroutine call");
        let sub = subs.get(&block_id).expect("Unanalyzed subroutine");
        self.analyze_sub_call(&sub.effects);
    }

    fn analyze_set_sp(&mut self, expr: &Expr) {
        let label = self.stack_label(0, self.sp);
        let value = self.analyze_expr(expr);
        self.set_value(label, value);
        self.sp += 1;
    }

    fn analyze_timer(&mut self, arg: &TimerArgs) {
        self.analyze_expr(&arg.duration);
        self.analyze_reference(ValueKind::Event, &arg.event);
    }

    fn analyze_sub_call(&mut self, effects: &SubroutineEffects) {
        // Tag live values which match the inputs for the subroutines
        for (&label, kind) in effects.input_kinds.iter() {
            let label = self.relative_label(label);
            if let Some(value) = self.values.get(&label) {
                let duplicate = value.clone();
                self.add_reference(kind.clone(), duplicate);
            }
        }

        // Kill any values killed by the subroutine
        for &killed in effects.killed.iter() {
            self.killed.insert(killed);
            self.values.remove(&killed);
        }

        // Analyze each output value with respect to the current state
        let mut output_values = HashMap::<Label, LiveValue>::new();
        for &(label, value) in effects.outputs.iter() {
            let resolved = self.resolve_output(value);
            match output_values.entry(label) {
                hash_map::Entry::Occupied(mut occupied) => {
                    occupied.get_mut().append(resolved);
                }
                hash_map::Entry::Vacant(vacant) => {
                    vacant.insert(resolved);
                }
            }
        }
        for (label, value) in output_values {
            self.values.insert(label, value);
        }
    }

    /// Resolves a subroutine output by replacing undefined references.
    fn resolve_output(&mut self, value: Value) -> LiveValue {
        match value {
            Value::Offset(offset) => Value::Offset(offset).into(),
            Value::Undefined(label) => {
                let label = self.relative_label(label);
                if let Some(value) = self.values.get(&label) {
                    value.clone()
                } else {
                    Value::Undefined(label).into()
                }
            }
        }
    }

    /// Analyzes an expression and produces a `LiveValue` for it.
    fn analyze_expr(&mut self, expr: &Expr) -> LiveValue {
        match expr {
            Expr::AddressOf(ip) => self.analyze_address_of(*ip),
            Expr::Stack(i) => self.resolve_label(self.stack_label(0, *i)),
            Expr::ParentStack(i) => self.resolve_label(self.stack_label(-1, *i)),
            Expr::Variable(index) => self.analyze_variable(index),
            Expr::Result1 => self.resolve_label(Label::Result1),
            Expr::Result2 => self.resolve_label(Label::Result2),
            Expr::Obj(obj) => self.analyze_obj(&**obj),
            Expr::ArrayElement(arr) => self.analyze_array_element(arr),

            Expr::Equal(op)
            | Expr::NotEqual(op)
            | Expr::Less(op)
            | Expr::LessEqual(op)
            | Expr::Greater(op)
            | Expr::GreaterEqual(op)
            | Expr::Add(op)
            | Expr::Subtract(op)
            | Expr::Multiply(op)
            | Expr::Divide(op)
            | Expr::Modulo(op)
            | Expr::BitAnd(op)
            | Expr::BitOr(op)
            | Expr::BitXor(op)
            | Expr::AddAssign(op)
            | Expr::SubtractAssign(op)
            | Expr::MultiplyAssign(op)
            | Expr::DivideAssign(op)
            | Expr::ModuloAssign(op)
            | Expr::BitAndAssign(op)
            | Expr::BitOrAssign(op)
            | Expr::BitXorAssign(op) => self.analyze_binary_op(op),

            Expr::Not(e)
            | Expr::Flag(e)
            | Expr::Pad(e)
            | Expr::Battery(e)
            | Expr::Item(e)
            | Expr::Atc(e)
            | Expr::Map(e)
            | Expr::ActorName(e)
            | Expr::ItemName(e)
            | Expr::Time(e)
            | Expr::StickerName(e)
            | Expr::Random(e)
            | Expr::Sin(e)
            | Expr::Cos(e) => {
                self.analyze_expr(e);
                LiveValue::Other
            }

            Expr::Imm16(_)
            | Expr::Imm32(_)
            | Expr::Money
            | Expr::Rank
            | Expr::Exp
            | Expr::Level
            | Expr::Hold
            | Expr::CurrentSuit
            | Expr::Scrap
            | Expr::CurrentAtc
            | Expr::Use
            | Expr::Hit => LiveValue::Other,
        }
    }

    fn analyze_binary_op(&mut self, op: &BinaryOp) -> LiveValue {
        match (self.analyze_expr(&op.lhs), self.analyze_expr(&op.rhs)) {
            (LiveValue::Other, LiveValue::Other) => LiveValue::Other,

            // If Offset(0) is involved in a calculation with a value, that value is being
            // interpreted as a file offset
            (lhs, LiveValue::Value(Value::Offset(0))) => LiveValue::Deref(lhs.into()),
            (LiveValue::Value(Value::Offset(0)), rhs) => LiveValue::Deref(rhs.into()),

            (lhs, LiveValue::Other) => lhs,
            (LiveValue::Other, rhs) => rhs,

            (_, _) => LiveValue::Other,
        }
    }

    fn analyze_address_of(&mut self, ip: Ip) -> LiveValue {
        if let Ip::Offset(off) = ip {
            LiveValue::Value(Value::Offset(off))
        } else {
            panic!("AddressOf expression does not reference an offset");
        }
    }

    fn analyze_variable(&mut self, index: &Expr) -> LiveValue {
        if let Some(index) = index.value() {
            self.resolve_label(Label::Variable(index as i16))
        } else {
            self.analyze_expr(index);
            LiveValue::Other
        }
    }

    fn analyze_obj(&mut self, expr: &ObjExpr) -> LiveValue {
        match expr {
            ObjExpr::Anim(arg)
            | ObjExpr::Dir(arg)
            | ObjExpr::PosX(arg)
            | ObjExpr::PosY(arg)
            | ObjExpr::PosZ(arg)
            | ObjExpr::Unk235(arg)
            | ObjExpr::Unk247(arg)
            | ObjExpr::Unk248(arg) => {
                self.analyze_expr(&arg.obj);
            }
            ObjExpr::DirTo(arg) | ObjExpr::Distance(arg) => {
                self.analyze_reference(ValueKind::ObjPair, &arg.address);
            }
            ObjExpr::BoneX(arg)
            | ObjExpr::BoneY(arg)
            | ObjExpr::BoneZ(arg)
            | ObjExpr::Unk249(arg)
            | ObjExpr::Unk250(arg) => {
                self.analyze_reference(ValueKind::ObjBone, &arg.address);
            }
        }
        LiveValue::Other
    }

    fn analyze_array_element(&mut self, arr: &ArrayElementExpr) -> LiveValue {
        self.analyze_expr(&arr.element_type);
        self.analyze_expr(&arr.index);
        let array_kind = ArrayKind::from_expr(&arr.element_type);
        let address = self.analyze_expr(&arr.address);
        self.add_reference(ValueKind::Array(array_kind), address.clone());
        LiveValue::ArrayElement(address.into())
    }

    fn analyze_set(&mut self, target: &SetExpr, expr: &Expr) {
        let value = self.analyze_expr(expr);
        let label = match target {
            SetExpr::Stack(i) => self.stack_label(0, *i),
            SetExpr::Variable(e) => {
                if let Some(val) = e.value() {
                    Label::Variable(val as i16)
                } else {
                    self.analyze_expr(e);
                    return;
                }
            }
            SetExpr::Result1 => Label::Result1,
            SetExpr::Result2 => Label::Result2,
            SetExpr::Pad(_) => {
                // Setting pad[7] is the only legal assignment, so assume the operand is an array
                self.add_reference(ValueKind::Array(ArrayKind::I16), value);
                return;
            }

            SetExpr::Flag(e)
            | SetExpr::Battery(e)
            | SetExpr::Item(e)
            | SetExpr::Atc(e)
            | SetExpr::Time(e) => {
                self.analyze_expr(e);
                return;
            }

            SetExpr::Money
            | SetExpr::Rank
            | SetExpr::Exp
            | SetExpr::Level
            | SetExpr::CurrentSuit
            | SetExpr::Scrap
            | SetExpr::CurrentAtc => return,
        };
        self.set_value(label, value);
    }

    /// Sets the value of a label and kills the old one.
    fn set_value(&mut self, label: Label, value: LiveValue) {
        self.killed.insert(label);
        self.values.insert(label, value);
    }

    /// Analyzes an expression and adds it to the reference list.
    fn analyze_reference(&mut self, kind: ValueKind, expr: &Expr) {
        let value = self.analyze_expr(expr);
        self.add_reference(kind, value);
    }

    /// Adds a value to the reference list.
    fn add_reference(&mut self, kind: ValueKind, value: LiveValue) {
        match value {
            LiveValue::Value(value) => self.references.push((kind, value)),
            LiveValue::Union(values) => {
                self.references.extend(values.into_iter().map(|v| (kind.clone(), v)));
            }
            LiveValue::Deref(target) => {
                // Dereferenced values come from an IP array
                self.add_reference(ValueKind::Array(ArrayKind::Ip(kind.into())), *target);
            }
            LiveValue::ArrayElement(arr) => {
                if let ValueKind::Array(_) = kind {
                    self.add_reference(kind, *arr);
                }
            }
            LiveValue::Other => (),
        }
    }

    /// Gets the current value assigned to `label` (or `Undefined` if none).
    fn resolve_label(&mut self, label: Label) -> LiveValue {
        if let Some(value) = self.values.get(&label) {
            value.clone()
        } else {
            Value::Undefined(label).into()
        }
    }

    /// Creates a label for a value in a stack frame.
    fn stack_label(&self, bp_offset: i16, sp: u8) -> Label {
        let bp = self.sp_stack.len() as i16 + bp_offset;
        Label::Stack(bp, sp)
    }

    /// Adjusts `label` to be relative to the current stack frame.
    fn relative_label(&self, label: Label) -> Label {
        match label {
            Label::Stack(bp, sp) => self.stack_label(bp, sp),
            label => label,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::value::DefId;
    use super::*;

    fn contains_label(defs: &DefinitionMap, set: &HashSet<DefId>, label: Label) -> bool {
        set.iter().any(|d| defs[*d].label == label)
    }

    #[test]
    fn test_analyze_const_value() {
        let mut state = LiveState::new();
        let expr16 = Expr::Imm16(1234);
        assert_eq!(LiveValue::Other, state.analyze_expr(&expr16));
        let expr32 = Expr::Imm32(12345678);
        assert_eq!(LiveValue::Other, state.analyze_expr(&expr32));
    }

    #[test]
    fn test_analyze_offset() {
        let mut state = LiveState::new();
        let expr = Expr::AddressOf(Ip::Offset(12345678));
        assert_eq!(Value::Offset(12345678), state.analyze_expr(&expr).value());
    }

    #[test]
    fn test_analyze_stack() {
        let mut state = LiveState::new();
        let expr = Expr::Stack(123);
        assert_eq!(Value::Undefined(Label::Stack(0, 123)), state.analyze_expr(&expr).value(),);
    }

    #[test]
    fn test_analyze_parent_stack() {
        let mut state = LiveState::new();
        state.sp_stack.push(3);
        state.sp_stack.push(5);
        let expr = Expr::ParentStack(1);
        assert_eq!(Value::Undefined(Label::Stack(1, 1)), state.analyze_expr(&expr).value());
    }

    #[test]
    fn test_analyze_variable() {
        let mut state = LiveState::new();
        let expr = Expr::Variable(Expr::Imm32(123).into());
        let label = Label::Variable(123);
        assert_eq!(Value::Undefined(label), state.analyze_expr(&expr).value());
    }

    #[test]
    fn test_analyze_result() {
        let mut state = LiveState::new();
        assert_eq!(Value::Undefined(Label::Result1), state.analyze_expr(&Expr::Result1).value(),);
        assert_eq!(Value::Undefined(Label::Result2), state.analyze_expr(&Expr::Result2).value(),);
    }

    #[test]
    fn test_analyze_set() {
        let commands: &[Command] = &[
            SetArgs::new(SetExpr::Stack(0), Expr::AddressOf(Ip::Offset(123))).into(),
            SetArgs::new(SetExpr::from_var(2), Expr::AddressOf(Ip::Offset(123))).into(),
            SetArgs::new(SetExpr::Result1, Expr::AddressOf(Ip::Offset(123))).into(),
            SetArgs::new(SetExpr::Result2, Expr::AddressOf(Ip::Offset(123))).into(),
            SetArgs::new(SetExpr::Exp, Expr::from_var(42)).into(),
            SetArgs::new(SetExpr::Battery(Expr::from_var(43)), Expr::AddressOf(Ip::Offset(123)))
                .into(),
        ];

        let mut state = LiveState::new();
        let subs = SubroutineInfoMap::new();
        for cmd in commands {
            state.analyze_command(cmd, &subs, &[]);
        }

        let mut defs = DefinitionMap::with_key();
        let block = state.into_block(BlockId::new(0), &mut defs);
        assert!(block.killed.contains(&Label::Stack(0, 0)));
        assert!(block.killed.contains(&Label::Variable(2)));
        assert!(block.killed.contains(&Label::Result1));
        assert!(block.killed.contains(&Label::Result2));
        assert!(contains_label(&defs, &block.generated, Label::Stack(0, 0)));
        assert!(contains_label(&defs, &block.generated, Label::Variable(2)));
        assert!(contains_label(&defs, &block.generated, Label::Result1));
        assert!(contains_label(&defs, &block.generated, Label::Result2));
        assert_eq!(block.generated, block.outputs);
    }

    #[test]
    fn test_analyze_set_sp() {
        let commands: &[Command] = &[
            Command::SetSp(Expr::AddressOf(Ip::Offset(123)).into()),
            Command::SetSp(Expr::AddressOf(Ip::Offset(456)).into()),
            Command::PushBp,
            Command::SetSp(Expr::AddressOf(Ip::Offset(789)).into()),
            Command::PopBp,
        ];

        let mut state = LiveState::new();
        let subs = SubroutineInfoMap::new();
        for cmd in commands {
            state.analyze_command(cmd, &subs, &[]);
        }

        let mut defs = DefinitionMap::with_key();
        let block = state.into_block(BlockId::new(0), &mut defs);
        assert!(block.killed.contains(&Label::Stack(0, 0)));
        assert!(block.killed.contains(&Label::Stack(0, 1)));
        assert!(block.killed.contains(&Label::Stack(1, 0)));
        assert!(contains_label(&defs, &block.generated, Label::Stack(0, 0)));
        assert!(contains_label(&defs, &block.generated, Label::Stack(0, 1)));
        assert!(!contains_label(&defs, &block.generated, Label::Stack(1, 0)));
    }

    #[test]
    fn test_resolve_label() {
        let commands: &[Command] = &[
            Command::Detach(Expr::from_var(42).into()),
            SetArgs::new(SetExpr::from_var(42), Expr::AddressOf(Ip::Offset(123))).into(),
            Command::Detach(Expr::from_var(42).into()),
        ];

        let mut state = LiveState::new();
        let subs = SubroutineInfoMap::new();
        for cmd in commands {
            state.analyze_command(cmd, &subs, &[]);
        }

        let mut defs = DefinitionMap::with_key();
        let block = state.into_block(BlockId::new(0), &mut defs);
        assert!(block.killed.contains(&Label::Variable(42)));
        assert!(contains_label(&defs, &block.generated, Label::Variable(42)));
        assert!(contains_label(&defs, &block.outputs, Label::Variable(42)));
    }
}
