use cosmwasm_std::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, JsonSchema)]
pub struct TimeRange {
    pub start: u64,
    pub finish: u64,
    pub inverse: bool,
}

impl Display for TimeRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.inverse {
            if self.start != 0 && self.finish != 0 {
                write!(f, "(~ {}, {} ~)", self.start, self.finish)
            } else if self.start == 0 {
                write!(f, "{} ~)", self.finish)
            } else {
                write!(f, "(~ {})", self.start)
            }
        }
        /* not inverse */
        else if self.start != 0 && self.finish != 0 {
            write!(f, "({} ~ {})", self.start, self.finish)
        } else if self.start == 0 {
            write!(f, "(~ {})", self.finish)
        } else {
            write!(f, "({} ~)", self.start)
        }
    }
}

impl TimeRange {
    pub fn period(&self) -> u64 {
        if self.inverse {
            0
        } else {
            self.finish - self.start
        }
    }

    pub fn is_in_range(&self, env: &Env) -> bool {
        if self.inverse {
            if self.start == 0 {
                return self.finish < env.block.time.seconds();
            }
            if self.finish == 0 {
                return env.block.time.seconds() < self.start;
            }
            env.block.time.seconds() < self.start || self.finish <= env.block.time.seconds()
        } else {
            if self.start == 0 {
                return env.block.time.seconds() < self.finish;
            }
            if self.finish == 0 {
                return self.start < env.block.time.seconds();
            }
            self.start <= env.block.time.seconds() && env.block.time.seconds() < self.finish
        }
    }
}

impl From<(u64, u64, bool)> for TimeRange {
    fn from((start, finish, inverse): (u64, u64, bool)) -> Self {
        Self {
            start,
            finish,
            inverse,
        }
    }
}

impl From<(u64, u64)> for TimeRange {
    fn from((start, finish): (u64, u64)) -> Self {
        Self {
            start,
            finish,
            inverse: false,
        }
    }
}
