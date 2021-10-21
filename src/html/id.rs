use std::{fmt::Display, ops::Add};

use markup::Render;

#[derive(Debug)]
pub struct Id(String);

impl Id {
    pub const fn new(id: String) -> Self {
        Self(id)
    }

    pub fn with_pound(&self) -> impl Display + Render + '_ {
        struct Pound<'a>(&'a Id);

        impl<'a> Display for Pound<'a> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("#")?;
                f.write_str(&self.0.0)
            }
        }

        impl<'a> Render for Pound<'a> {
            fn render(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
                writer.write_str("#")?;
                writer.write_str(&self.0.0)
            }
        }

        Pound(self)
    }
}

impl Add for Id {
    type Output = Id;

    fn add(self, other: Self) -> Self {
        Self(format!("{}.{}", self.0, other.0))
    }
}

impl Add<Id> for &Id {
    type Output = Id;

    fn add(self, other: Id) -> Self::Output {
        Id::new(format!("{}.{}", self.0, other.0))
    }
}

impl Render for Id {
    fn render(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        writer.write_str(&self.0)
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
