use log::debug;
use std::collections::{HashMap, HashSet};
use unplug::event::command::ReadType;
use unplug::event::expr::ObjExpr;
use unplug::event::{
    Block, BlockId, CodeBlock, Command, DataBlock, Expr, Pointer, Script, SetExpr,
};

/// Compares two expressions for equivalence but ignores pointers.
fn compare_exprs(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (Expr::Equal(a), Expr::Equal(b))
        | (Expr::NotEqual(a), Expr::NotEqual(b))
        | (Expr::Less(a), Expr::Less(b))
        | (Expr::LessEqual(a), Expr::LessEqual(b))
        | (Expr::Greater(a), Expr::Greater(b))
        | (Expr::GreaterEqual(a), Expr::GreaterEqual(b))
        | (Expr::Add(a), Expr::Add(b))
        | (Expr::Subtract(a), Expr::Subtract(b))
        | (Expr::Multiply(a), Expr::Multiply(b))
        | (Expr::Divide(a), Expr::Divide(b))
        | (Expr::Modulo(a), Expr::Modulo(b))
        | (Expr::BitAnd(a), Expr::BitAnd(b))
        | (Expr::BitOr(a), Expr::BitOr(b))
        | (Expr::BitXor(a), Expr::BitXor(b))
        | (Expr::AddAssign(a), Expr::AddAssign(b))
        | (Expr::SubtractAssign(a), Expr::SubtractAssign(b))
        | (Expr::MultiplyAssign(a), Expr::MultiplyAssign(b))
        | (Expr::DivideAssign(a), Expr::DivideAssign(b))
        | (Expr::ModuloAssign(a), Expr::ModuloAssign(b))
        | (Expr::BitAndAssign(a), Expr::BitAndAssign(b))
        | (Expr::BitOrAssign(a), Expr::BitOrAssign(b))
        | (Expr::BitXorAssign(a), Expr::BitXorAssign(b)) => {
            compare_exprs(&a.lhs, &b.lhs) && compare_exprs(&a.rhs, &b.rhs)
        }

        (Expr::Not(a), Expr::Not(b))
        | (Expr::Flag(a), Expr::Flag(b))
        | (Expr::Variable(a), Expr::Variable(b))
        | (Expr::Pad(a), Expr::Pad(b))
        | (Expr::Battery(a), Expr::Battery(b))
        | (Expr::Item(a), Expr::Item(b))
        | (Expr::Atc(a), Expr::Atc(b))
        | (Expr::Map(a), Expr::Map(b))
        | (Expr::ActorName(a), Expr::ActorName(b))
        | (Expr::ItemName(a), Expr::ItemName(b))
        | (Expr::Time(a), Expr::Time(b))
        | (Expr::StickerName(a), Expr::StickerName(b))
        | (Expr::Random(a), Expr::Random(b))
        | (Expr::Sin(a), Expr::Sin(b))
        | (Expr::Cos(a), Expr::Cos(b)) => compare_exprs(a, b),

        (Expr::Imm16(a), Expr::Imm16(b)) => a == b,
        (Expr::Imm32(a), Expr::Imm32(b)) => a == b,

        (Expr::AddressOf(_), Expr::AddressOf(_)) => true,

        (Expr::Stack(a), Expr::Stack(b)) | (Expr::ParentStack(a), Expr::ParentStack(b)) => a == b,

        (Expr::Result1, Expr::Result1)
        | (Expr::Result2, Expr::Result2)
        | (Expr::Rank, Expr::Rank)
        | (Expr::Exp, Expr::Exp)
        | (Expr::Level, Expr::Level)
        | (Expr::Hold, Expr::Hold)
        | (Expr::Money, Expr::Money)
        | (Expr::CurrentSuit, Expr::CurrentSuit)
        | (Expr::Scrap, Expr::Scrap)
        | (Expr::CurrentAtc, Expr::CurrentAtc)
        | (Expr::Use, Expr::Use)
        | (Expr::Hit, Expr::Hit) => true,

        (Expr::Obj(a), Expr::Obj(b)) => match (&**a, &**b) {
            (ObjExpr::Anim(a), ObjExpr::Anim(b))
            | (ObjExpr::Dir(a), ObjExpr::Dir(b))
            | (ObjExpr::PosX(a), ObjExpr::PosX(b))
            | (ObjExpr::PosY(a), ObjExpr::PosY(b))
            | (ObjExpr::PosZ(a), ObjExpr::PosZ(b))
            | (ObjExpr::Unk235(a), ObjExpr::Unk235(b))
            | (ObjExpr::Unk247(a), ObjExpr::Unk247(b))
            | (ObjExpr::Unk248(a), ObjExpr::Unk248(b)) => compare_exprs(&a.obj, &b.obj),

            (ObjExpr::BoneX(a), ObjExpr::BoneX(b))
            | (ObjExpr::BoneY(a), ObjExpr::BoneY(b))
            | (ObjExpr::BoneZ(a), ObjExpr::BoneZ(b))
            | (ObjExpr::Unk249(a), ObjExpr::Unk249(b))
            | (ObjExpr::Unk250(a), ObjExpr::Unk250(b)) => compare_exprs(&a.address, &b.address),

            (ObjExpr::DirTo(a), ObjExpr::DirTo(b))
            | (ObjExpr::Distance(a), ObjExpr::Distance(b)) => compare_exprs(&a.address, &b.address),

            _ => false,
        },

        (Expr::ArrayElement(a), Expr::ArrayElement(b)) => {
            compare_exprs(&a.element_type, &b.element_type)
                && compare_exprs(&a.index, &b.index)
                && compare_exprs(&a.address, &b.address)
        }

        _ => false,
    }
}

/// Compares two lists of expressions for equivalence but ignores pointers.
fn compare_many_exprs(a: &[Expr], b: &[Expr]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| compare_exprs(a, b))
}

/// Compares two `set()` expressions for equivalence but ignores pointers.
fn compare_set_exprs(a: &SetExpr, b: &SetExpr) -> bool {
    match (a, b) {
        (SetExpr::Stack(a), SetExpr::Stack(b)) => a == b,

        (SetExpr::Flag(a), SetExpr::Flag(b))
        | (SetExpr::Variable(a), SetExpr::Variable(b))
        | (SetExpr::Pad(a), SetExpr::Pad(b))
        | (SetExpr::Battery(a), SetExpr::Battery(b))
        | (SetExpr::Item(a), SetExpr::Item(b))
        | (SetExpr::Atc(a), SetExpr::Atc(b))
        | (SetExpr::Time(a), SetExpr::Time(b)) => compare_exprs(a, b),

        (SetExpr::Result1, SetExpr::Result1)
        | (SetExpr::Result2, SetExpr::Result2)
        | (SetExpr::Money, SetExpr::Money)
        | (SetExpr::Rank, SetExpr::Rank)
        | (SetExpr::Exp, SetExpr::Exp)
        | (SetExpr::Level, SetExpr::Level)
        | (SetExpr::CurrentSuit, SetExpr::CurrentSuit)
        | (SetExpr::Scrap, SetExpr::Scrap)
        | (SetExpr::CurrentAtc, SetExpr::CurrentAtc) => true,

        _ => false,
    }
}

/// Compares two commands for equivalence but ignores pointers.
fn compare_commands(a: &Command, b: &Command) -> bool {
    match (a, b) {
        (Command::Abort, Command::Abort)
        | (Command::Return, Command::Return)
        | (Command::Goto(_), Command::Goto(_))
        | (Command::EndIf(_), Command::EndIf(_))
        | (Command::Break(_), Command::Break(_))
        | (Command::Run(_), Command::Run(_))
        | (Command::PushBp, Command::PushBp)
        | (Command::PopBp, Command::PopBp) => true,

        (Command::Set(a), Command::Set(b)) => {
            compare_exprs(&a.value, &b.value) && compare_set_exprs(&a.target, &b.target)
        }

        (Command::If(a), Command::If(b))
        | (Command::Elif(a), Command::Elif(b))
        | (Command::Case(a), Command::Case(b))
        | (Command::Expr(a), Command::Expr(b))
        | (Command::While(a), Command::While(b)) => compare_exprs(&a.condition, &b.condition),

        (Command::SetSp(a), Command::SetSp(b))
        | (Command::Detach(a), Command::Detach(b))
        | (Command::Kill(a), Command::Kill(b)) => compare_exprs(a, b),

        (Command::Anim(a), Command::Anim(b))
        | (Command::Anim1(a), Command::Anim1(b))
        | (Command::Anim2(a), Command::Anim2(b)) => {
            compare_exprs(&a.obj, &b.obj) && compare_many_exprs(&a.values, &b.values)
        }

        (Command::Attach(a), Command::Attach(b)) => {
            compare_exprs(&a.obj, &b.obj) && compare_exprs(&a.event, &b.event)
        }

        (Command::Born(a), Command::Born(b)) => {
            compare_exprs(&a.val1, &b.val1)
                && compare_exprs(&a.val2, &b.val2)
                && compare_exprs(&a.val3, &b.val3)
                && compare_exprs(&a.val4, &b.val4)
                && compare_exprs(&a.val5, &b.val5)
                && compare_exprs(&a.val6, &b.val6)
                && compare_exprs(&a.val7, &b.val7)
                && compare_exprs(&a.val8, &b.val8)
                && compare_exprs(&a.val9, &b.val9)
                && compare_exprs(&a.event, &b.event)
        }

        (Command::Call(a), Command::Call(b)) => {
            compare_exprs(&a.obj, &b.obj) && compare_many_exprs(&a.args, &b.args)
        }

        (Command::Read(a), Command::Read(b)) => match (&**a, &**b) {
            (ReadType::Anim(a), ReadType::Anim(b)) => {
                compare_exprs(&a.obj, &b.obj) && compare_exprs(&a.path, &b.path)
            }
            (ReadType::Sfx(a), ReadType::Sfx(b)) => {
                compare_exprs(&a.obj, &b.obj) && compare_exprs(&a.path, &b.path)
            }
            _ => false,
        },

        (Command::Timer(a), Command::Timer(b)) => {
            compare_exprs(&a.duration, &b.duration) && compare_exprs(&a.event, &b.event)
        }

        (Command::Movie(a), Command::Movie(b)) => {
            compare_exprs(&a.path, &b.path)
                && compare_exprs(&a.val1, &b.val1)
                && compare_exprs(&a.val2, &b.val2)
                && compare_exprs(&a.val3, &b.val3)
                && compare_exprs(&a.val4, &b.val4)
                && compare_exprs(&a.val5, &b.val5)
        }

        (Command::Lib(a), Command::Lib(b)) => a == b,
        (Command::Msg(a), Command::Msg(b)) | (Command::Select(a), Command::Select(b)) => a == b,
        (Command::PrintF(a), Command::PrintF(b)) => a == b,

        // HACK: these typically don't contain pointers, so it works to just do equality comparison
        // instead of writing out all the pattern matching code
        (Command::Camera(a), Command::Camera(b)) => a == b,
        (Command::Check(a), Command::Check(b)) => a == b,
        (Command::Color(a), Command::Color(b)) => a == b,
        (Command::Dir(a), Command::Dir(b)) => a == b,
        (Command::MDir(a), Command::MDir(b)) => a == b,
        (Command::Disp(a), Command::Disp(b)) => a == b,
        (Command::Light(a), Command::Light(b)) => a == b,
        (Command::Menu(a), Command::Menu(b)) => a == b,
        (Command::Move(a), Command::Move(b)) => a == b,
        (Command::MoveTo(a), Command::MoveTo(b)) => a == b,
        (Command::Pos(a), Command::Pos(b)) => a == b,
        (Command::Ptcl(a), Command::Ptcl(b)) => a == b,
        (Command::Scale(a), Command::Scale(b)) => a == b,
        (Command::MScale(a), Command::MScale(b)) => a == b,
        (Command::Scrn(a), Command::Scrn(b)) => a == b,
        (Command::Sfx(a), Command::Sfx(b)) => a == b,
        (Command::Wait(a), Command::Wait(b)) => a == b,
        (Command::Warp(a), Command::Warp(b)) => a == b,
        (Command::Win(a), Command::Win(b)) => a == b,

        _ => false,
    }
}

/// Throws an assertion failure if two subroutines do not structurally match. Pointers will be
/// ignored.
fn assert_code_matches(a: &CodeBlock, b: &CodeBlock) {
    assert_eq!(a.next_block.is_some(), b.next_block.is_some());
    assert_eq!(a.else_block.is_some(), b.else_block.is_some());
    assert_eq!(a.commands.len(), b.commands.len());
    for (a, b) in a.commands.iter().zip(&b.commands) {
        assert!(compare_commands(a, b), "a = {a:?}, b = {b:?}");
    }
}

/// Throws an assertion failure if two subroutines do not structurally match. Pointers will be
/// ignored.
fn assert_data_matches(a: &DataBlock, b: &DataBlock) {
    match (a, b) {
        (DataBlock::PtrArray(a), DataBlock::PtrArray(b)) => assert_eq!(a.len(), b.len()),
        _ => assert_eq!(a, b),
    }
}

/// Throws an assertion failure if two subroutines do not structurally match. Pointers will be
/// ignored.
fn assert_blocks_match(a: &Block, b: &Block) {
    match (a, b) {
        (Block::Placeholder, Block::Placeholder) => (),
        (Block::Code(a), Block::Code(b)) => assert_code_matches(a, b),
        (Block::Data(a), Block::Data(b)) => assert_data_matches(a, b),
        _ => panic!("block types do not match"),
    }
}

/// Throws an assertion failure if two subroutines do not structurally match. Pointers will be
/// ignored.
fn assert_subroutines_match(
    script1: &Script,
    sub1: BlockId,
    script2: &Script,
    sub2: BlockId,
    visited: &mut HashSet<BlockId>,
) {
    assert_eq!(sub1, sub2);
    if !visited.insert(sub1) {
        return;
    }
    let b1 = script1.block(sub1);
    let b2 = script2.block(sub2);
    assert_blocks_match(b1, b2);
    if let (Block::Code(code1), Block::Code(code2)) = (b1, b2) {
        if let (Some(Pointer::Block(n1)), Some(Pointer::Block(n2))) =
            (code1.next_block, code2.next_block)
        {
            assert_subroutines_match(script1, n1, script2, n2, visited);
        }
        if let (Some(Pointer::Block(e1)), Some(Pointer::Block(e2))) =
            (code1.else_block, code2.else_block)
        {
            assert_subroutines_match(script1, e1, script2, e2, visited);
        }
    }
}

/// Throws an assertion failure if two scripts do not structurally match. Pointers inside scripts
/// will be ignored.
pub fn assert_scripts_match(script1: &Script, script2: &Script) {
    assert_eq!(script1.len(), script2.len());

    // Sort subroutines by offset to line them up. The script writer sorts blobs by their offsets in
    // the original file, so this actually works.
    let layout1 = script1.layout().unwrap();
    let layout2 = script2.layout().unwrap();
    let mut subs1 = layout1.subroutines().keys().copied().collect::<Vec<_>>();
    let mut subs2 = layout2.subroutines().keys().copied().collect::<Vec<_>>();
    assert_eq!(subs1.len(), subs2.len());
    let offsets1 =
        layout1.block_offsets().iter().map(|loc| (loc.id, loc.offset)).collect::<HashMap<_, _>>();
    let offsets2 =
        layout2.block_offsets().iter().map(|loc| (loc.id, loc.offset)).collect::<HashMap<_, _>>();
    subs1.sort_unstable_by_key(|a| offsets1.get(a).unwrap());
    subs2.sort_unstable_by_key(|a| offsets2.get(a).unwrap());

    let mut visited = HashSet::new();
    for (&sub1, &sub2) in subs1.iter().zip(&subs2) {
        assert_subroutines_match(script1, sub1, script2, sub2, &mut visited);
    }
    debug!("Compared {} blocks", visited.len());
}
