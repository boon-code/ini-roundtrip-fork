use core::{
    cmp::{max, min},
    error::Error,
    fmt,
};

use crate::{Item, from_utf8, parse};

pub struct ValuePreserve<'a> {
    pub pre: &'a str,
    pub value: &'a str,
    pub post: &'a str,
}

impl<'a> fmt::Display for ValuePreserve<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}{}", self.pre, self.value, self.post)
    }
}

impl<'a> ValuePreserve<'a> {
    pub fn trim_from_utf8(slice: &'a [u8]) -> Self {
        let value = from_utf8(slice);
        Self::trim(value)
    }

    pub fn trim_section_from_utf8(slice: &'a [u8]) -> Result<Self, ()> {
        let value = from_utf8(slice);
        Self::trim_section(value)
    }

    fn trim(txt: &'a str) -> Self {
        let pat = |chr: char| chr.is_ascii_whitespace();

        let trimmed_start = txt.trim_start_matches(pat);
        let trimmed = trimmed_start.trim_end_matches(pat);

        assert!(txt.len() >= trimmed_start.len());
        assert!(trimmed_start.len() >= trimmed.len());

        let pre_len = txt.len() - trimmed_start.len();
        let post_len = trimmed_start.len() - trimmed.len();

        assert!(post_len <= txt.len());
        assert!(pre_len + post_len + trimmed.len() == txt.len());

        let pre = &txt[..pre_len];
        let post = &txt[(txt.len() - post_len)..];

        Self {
            pre,
            value: trimmed,
            post,
        }
    }

    fn trim_section(txt: &'a str) -> Result<Self, ()> {
        let pat = |chr: char| chr.is_ascii_whitespace();

        let trimmed_start = txt.trim_start_matches(pat);
        let trimmed = trimmed_start.trim_end_matches(pat);

        assert!(txt.len() >= trimmed_start.len());
        assert!(trimmed_start.len() >= trimmed.len());

        let first = trimmed.chars().next();
        let last = trimmed.chars().last();

        match (first, last) {
            (Some('['), Some(']')) => {
                let pre_len = txt.len() - trimmed_start.len() + 1;
                let post_len = trimmed_start.len() - trimmed.len() + 1;
                let trimmed = &trimmed[1..trimmed.len() - 1];

                assert!(post_len <= txt.len());
                assert!(pre_len + post_len + trimmed.len() == txt.len());

                let val = Self::trim(&trimmed);

                let pre_len = pre_len + val.pre.len();
                let post_len = post_len + val.post.len();
                let trimmed_name = val.value;

                assert!(pre_len + post_len + trimmed_name.len() == txt.len());

                let pre = &txt[..pre_len];
                let post = &txt[(txt.len() - post_len)..];

                Ok(Self {
                    pre,
                    value: trimmed_name,
                    post,
                })
            }
            _ => Err(()),
        }
    }
}

pub struct PropWithCmt<'a> {
    pub key: ValuePreserve<'a>,
    pub val: Option<ValuePreserve<'a>>,
    pub cmt: Option<&'a str>,
    pub raw: &'a str,
    pub next: &'a [u8],
}

impl<'a> PropWithCmt<'a> {
    pub fn parse(s: &'a [u8]) -> Self {
        let eol_or_eq = parse::find_nl_chr(s, b'=');
        let (is_key_only, key_len, key_cmt) = Self::parse_key_with_cmt(s, eol_or_eq);
        let key = ValuePreserve::trim_from_utf8(&s[..key_len]);
        if is_key_only {
            // Key only case
            let next = &s[eol_or_eq..];
            Self {
                key,
                val: None,
                cmt: key_cmt,
                raw: from_utf8(&s[..eol_or_eq]),
                next,
            }
        } else {
            // Key + value case
            let val_start = &s[eol_or_eq + 1..];

            let (i_val, i_nl, cmt) = Self::parse_value_with_cmt(val_start);
            let val = ValuePreserve::trim_from_utf8(&val_start[..i_val]);
            let next = &val_start[i_nl..];

            Self {
                key,
                val: Some(val),
                cmt,
                raw: from_utf8(&s[..eol_or_eq + i_nl + 1]),
                next,
            }
        }
    }

    fn parse_value_with_cmt(slice: &[u8]) -> (usize, usize, Option<&str>) {
        let c1 = parse::find_nl_chr(slice, b'#');
        let c2 = parse::find_nl_chr(slice, b';');
        let c_start = min(c1, c2);
        let maybe_nl = max(c1, c2);

        let nl = match slice.get(maybe_nl) {
            Some(b'\n') | Some(b'\r') | None => maybe_nl,
            _ => parse::find_nl(&slice[maybe_nl..]) + maybe_nl,
        };

        let cmt = if c_start < nl {
            let cmt = from_utf8(&slice[c_start..nl]);
            Some(cmt)
        } else {
            None
        };

        (c_start, nl, cmt)
    }

    fn parse_key_with_cmt(slice: &[u8], eol_or_eq: usize) -> (bool, usize, Option<&str>) {
        let key_slice = &slice[..eol_or_eq];

        let c1 = parse::find_nl_chr(key_slice, b'#');
        let c2 = parse::find_nl_chr(key_slice, b';');
        let c_start = min(c1, c2);

        assert!(c_start <= eol_or_eq);

        if c_start < eol_or_eq {
            let cmt = from_utf8(&slice[c_start..eol_or_eq]);
            (true, c_start, Some(cmt))
        } else if slice.get(eol_or_eq) != Some(&b'=') {
            (true, eol_or_eq, None)
        } else {
            (false, eol_or_eq, None)
        }
    }

    pub fn to_item(self) -> Item<'a> {
        if self.val.is_none() && self.key.value.is_empty() {
            Item::Blank { raw: self.raw }
        } else {
            let val = if let Some(val) = self.val {
                Some(val.value)
            } else {
                None
            };
            Item::Property {
                key: self.key.value,
                val,
                raw: self.raw,
            }
        }
    }

    pub fn fmt_edit_value(&self, f: &mut fmt::Formatter<'_>, value: Option<&str>) -> fmt::Result {
        if value == self.val.as_ref().map(|x| x.value) {
            // no change necessary
            write!(f, "{}", self.raw)
        } else {
            let cmt = self.cmt.unwrap_or("");

            let (pre, post) = self
                .val
                .as_ref()
                .map(|x| (x.pre, x.post))
                .unwrap_or(("", ""));

            if let Some(value) = value {
                write!(f, "{}={pre}{}{post}{}", self.key, value, cmt)
            } else {
                write!(f, "{}{}", self.key, cmt)
            }
        }
    }

    pub fn edit_value(&'a self, value: Option<&'a str>) -> EditProp<'a> {
        EditProp { prop: self, value }
    }

    pub fn get_value(&'a self) -> Option<&'a str> {
        self.val.as_ref().map(|x| x.value)
    }
}

pub struct EditProp<'a> {
    prop: &'a PropWithCmt<'a>,
    value: Option<&'a str>,
}

impl<'a> fmt::Display for EditProp<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.prop.fmt_edit_value(f, self.value)
    }
}

#[derive(Debug)]
pub struct SectionError<'a> {
    pub error: &'a str,
    pub next: &'a [u8],
}

impl<'a> fmt::Display for SectionError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Malformed section: {}", self.error)
    }
}

impl<'a> Error for SectionError<'a> {}

pub struct SectionWithCmt<'a> {
    pub name: ValuePreserve<'a>,
    pub cmt: Option<&'a str>,
    pub next: &'a [u8],
    pub raw: &'a str,
}

impl<'a> SectionWithCmt<'a> {
    pub fn parse(s: &'a [u8]) -> Result<Self, SectionError<'a>> {
        let c1 = parse::find_nl_chr(s, b'#');
        let c2 = parse::find_nl_chr(s, b';');
        let c_start = min(c1, c2);
        let maybe_nl = max(c1, c2);

        let nl = match s.get(maybe_nl) {
            Some(b'\n') | Some(b'\r') | None => maybe_nl,
            _ => parse::find_nl(&s[maybe_nl..]) + maybe_nl,
        };

        let cmt = if c_start < nl {
            let cmt = from_utf8(&s[c_start..nl]);
            Some(cmt)
        } else {
            None
        };

        let next = &s[nl..];
        match ValuePreserve::trim_section_from_utf8(&s[..c_start]) {
            Ok(section) => Ok(Self {
                name: section,
                cmt,
                next,
                raw: from_utf8(&s[..nl]),
            }),
            Err(_) => Err(SectionError {
                error: from_utf8(&s[..nl]),
                next,
            }),
        }
    }

    pub fn to_item(self) -> Item<'a> {
        Item::Section {
            name: self.name.value,
            raw: self.raw,
        }
    }

    fn fmt_edit_value(&self, f: &mut fmt::Formatter<'_>, value: &str) -> Result<(), fmt::Error> {
        if value == self.name.value {
            write!(f, "{}", self.raw)
        } else {
            let cmt = self.cmt.unwrap_or("");
            write!(f, "{}{}{}{}", self.name.pre, value, self.name.post, cmt)
        }
    }

    pub fn edit_value(&'a self, value: &'a str) -> EditSection<'a> {
        EditSection {
            section: self,
            value,
        }
    }
}

pub struct EditSection<'a> {
    section: &'a SectionWithCmt<'a>,
    value: &'a str,
}

impl<'a> fmt::Display for EditSection<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.section.fmt_edit_value(f, self.value)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::format;

    use super::*;

    #[test]
    fn test_preserved_whitespace_value_trimmed() {
        let value = "notrim";
        let a = ValuePreserve::trim_from_utf8(value.as_bytes());
        assert_eq!(value, a.value);
        assert_eq!("", a.pre);
        assert_eq!("", a.post);
    }

    #[test]
    fn test_preserved_whitespace_value_start() {
        let value = " trim start";
        let a = ValuePreserve::trim_from_utf8(value.as_bytes());
        assert_eq!("trim start", a.value);
        assert_eq!(" ", a.pre);
        assert_eq!("", a.post);
    }

    #[test]
    fn test_preserved_whitespace_value_end() {
        let value = "trim end ";
        let a = ValuePreserve::trim_from_utf8(value.as_bytes());
        assert_eq!("trim end", a.value);
        assert_eq!("", a.pre);
        assert_eq!(" ", a.post);
    }

    #[test]
    fn test_preserved_whitespace_value_both() {
        let value = " trim both ";
        let a = ValuePreserve::trim_from_utf8(value.as_bytes());
        assert_eq!("trim both", a.value);
        assert_eq!(" ", a.pre);
        assert_eq!(" ", a.post);
    }

    #[test]
    fn test_parse_prop_simple() {
        let line = "key = value";
        let p = PropWithCmt::parse(line.as_bytes());
        assert_eq!("", p.key.pre);
        assert_eq!("key", p.key.value);
        assert_eq!(" ", p.key.post);
        let val = p.val.unwrap();
        assert_eq!(" ", val.pre);
        assert_eq!("value", val.value);
        assert_eq!("", val.post);
        assert!(p.cmt.is_none());
        assert_eq!(line, p.raw);
        assert!(p.next.is_empty());
    }

    #[test]
    fn test_parse_prop_full() {
        let line_nocmt = " key  =   value    ";
        let cmt = "# ffff fffd ggd == ffdd # fddd ;ff gg ";
        let line = format!("{line_nocmt}{cmt}");
        let p = PropWithCmt::parse(line.as_bytes());

        assert_eq!(" ", p.key.pre);
        assert_eq!("key", p.key.value);
        assert_eq!("  ", p.key.post);

        let val = p.val.unwrap();
        assert_eq!("   ", val.pre);
        assert_eq!("value", val.value);
        assert_eq!("    ", val.post);

        assert_eq!(cmt, p.cmt.unwrap());
        assert_eq!(line, p.raw);
        assert!(p.next.is_empty());
    }

    #[test]
    fn test_parse_multi_line() {
        let line = "key=value#comment\nbla";
        let p = PropWithCmt::parse(line.as_bytes());

        assert_eq!("\nbla".as_bytes(), p.next);

        assert_eq!("key", p.key.value);
        assert_eq!("value", p.val.unwrap().value);
        assert_eq!("#comment", p.cmt.unwrap());
    }

    #[test]
    fn test_edit_prop_preserve() {
        let line_in = " key  =   value     # Some ; complicated # comment\nnext";
        let line_out = " key  =   new value     # Some ; complicated # comment";

        let p = PropWithCmt::parse(line_in.as_bytes());
        let act = format!("{}", p.edit_value(Some("new value")));

        assert_eq!(line_out, &act);
        assert_eq!("\nnext".as_bytes(), p.next);
    }

    #[test]
    fn test_parse_section_simple() {
        let line = "[my name]";

        let s = SectionWithCmt::parse(line.as_bytes()).unwrap();

        assert_eq!("my name", s.name.value);
        assert_eq!("[", s.name.pre);
        assert_eq!("]", s.name.post);
    }

    #[test]
    fn test_parse_section_start() {
        let line = "[  my name]";

        let s = SectionWithCmt::parse(line.as_bytes()).unwrap();

        assert_eq!("my name", s.name.value);
        assert_eq!("[  ", s.name.pre);
        assert_eq!("]", s.name.post);
    }

    #[test]
    fn test_parse_section_end() {
        let line = "[my name  ]   ";

        let s = SectionWithCmt::parse(line.as_bytes()).unwrap();

        assert_eq!("my name", s.name.value);
        assert_eq!("[", s.name.pre);
        assert_eq!("  ]   ", s.name.post);
    }

    #[test]
    fn test_parse_section_full() {
        let cmt = "# asdjas []][;# bla";
        let line_nocmt = "[ my name  ]   ";
        let line = format!("{line_nocmt}{cmt}");

        let s = SectionWithCmt::parse(line.as_bytes()).unwrap();

        assert_eq!("my name", s.name.value);
        assert_eq!("[ ", s.name.pre);
        assert_eq!("  ]   ", s.name.post);
    }

    #[test]
    fn test_edit_section_preserve() {
        let line_in = "[ bla  ]    # Some ; complicated # comment\nnext";
        let line_out = "[ new value  ]    # Some ; complicated # comment";

        let sec = SectionWithCmt::parse(line_in.as_bytes()).unwrap();
        let act = format!("{}", sec.edit_value("new value"));

        assert_eq!(line_out, &act);
        assert_eq!("\nnext".as_bytes(), sec.next);
    }
}
