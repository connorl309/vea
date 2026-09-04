// Operand shapes.
//
// `Form` is the only thing an instruction row needs to say about its
// operands. The encoder reads two facts off it: how many register bytes go
// in the payload, and whether a trailing immediate follows. Immediate width
// is never declared - it's inferred from the value (see `encode::min_width`).

/// The syntactic shape of an instruction's operands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Form {
    /// `op`
    Nullary,
    /// `op rd`
    R,
    /// `op rd, rs`
    RR,
    /// `op rd, rs1, rs2`
    RRR,
    /// `op rd, #imm`
    RI,
    /// `op rd, rs, #imm`
    RRI,
    /// `op #imm`  (jump / branch / call targets)
    I,
    /// `op rd, [rb + disp]`  (loads and stores; disp is the immediate)
    RMem,
}

impl Form {
    /// (register operand count, has a trailing immediate)
    pub const fn shape(self) -> (u8, bool) {
        match self {
            Form::Nullary => (0, false),
            Form::R => (1, false),
            Form::RR => (2, false),
            Form::RRR => (3, false),
            Form::RI => (1, true),
            Form::RRI => (2, true),
            Form::I => (0, true),
            Form::RMem => (2, true),
        }
    }

    pub const fn reg_count(self) -> u8 {
        self.shape().0
    }

    pub const fn has_imm(self) -> bool {
        self.shape().1
    }
}
