/* See LICENSE file for copyright and license details. */
//! Small helpers shared by the other modules (a port of dwm's util.c/util.h).

/// Print `msg` to stderr and exit with status 1.
///
/// Like dwm's `die()`: if `msg` ends with a ':' the description of the last OS
/// error (errno) is appended.
pub fn die(msg: &str) -> ! {
    /* save errno before anything else can clobber it */
    let saved_errno = std::io::Error::last_os_error();

    if msg.ends_with(':') {
        eprintln!("{} {}", msg, saved_errno);
    } else {
        eprintln!("{}", msg);
    }
    std::process::exit(1);
}

/// Truncate `s` to at most `max_bytes` bytes without splitting a UTF-8
/// sequence. This mirrors the fixed-size `char[N]` buffers dwm uses for
/// window titles, the status text and the layout symbol.
pub fn truncate_utf8(s: &mut String, max_bytes: usize) {
    if s.len() > max_bytes {
        let mut i = max_bytes;
        while !s.is_char_boundary(i) {
            i -= 1;
        }
        s.truncate(i);
    }
}

/// `atoi()`: the leading integer of `s` after optional whitespace and sign,
/// 0 if there is none; saturating where C's is undefined.
pub fn atoi(s: &str) -> i32 {
    let s = s.trim_start_matches(C_SPACE);
    let (neg, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let mut n: i32 = 0;
    for b in digits.bytes().take_while(u8::is_ascii_digit) {
        n = n.saturating_mul(10).saturating_add((b - b'0') as i32);
    }
    if neg {
        -n
    } else {
        n
    }
}

/// The C whitespace `strtoul()`/`strtof()` skip.
const C_SPACE: [char; 6] = [' ', '\t', '\n', '\r', '\x0b', '\x0c'];

/// `strtoul(s, NULL, 10)`: the leading unsigned integer of `s` after optional
/// whitespace and sign, or `None` if there is no digit at all (where C
/// returns 0). Saturates instead of setting ERANGE; a negative number wraps
/// like C's does.
pub fn strtoul(s: &str) -> Option<u64> {
    let s = s.trim_start_matches(C_SPACE);
    let (neg, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let mut n: u64 = 0;
    let mut ndigits = 0;
    for b in digits.bytes().take_while(u8::is_ascii_digit) {
        n = n.saturating_mul(10).saturating_add((b - b'0') as u64);
        ndigits += 1;
    }
    if ndigits == 0 {
        None
    } else if neg {
        Some(n.wrapping_neg())
    } else {
        Some(n)
    }
}

/// `strtof(s, NULL)`: the leading decimal floating point number of `s` after
/// optional whitespace, or `None` if there is none (where C returns 0). The
/// hexadecimal, infinity and NaN forms C also accepts are not.
pub fn strtof(s: &str) -> Option<f32> {
    let s = s.trim_start_matches(C_SPACE);
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let mut ndigits = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
        ndigits += 1;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
            ndigits += 1;
        }
    }
    if ndigits == 0 {
        return None;
    }
    /* an exponent only counts if it has digits, like "1e" is 1 in C */
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    s[..i].parse().ok()
}

/// `BETWEEN(X, A, B)`: A <= X <= B.
#[allow(dead_code)]
#[inline]
pub fn between<T: PartialOrd>(x: T, a: T, b: T) -> bool {
    a <= x && x <= b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_keeps_char_boundaries() {
        let mut s = String::from("aé€😀");
        truncate_utf8(&mut s, 4); /* 'a' (1) + 'é' (2) = 3, '€' would need 3 more */
        assert_eq!(s, "aé");
        let mut t = String::from("abc");
        truncate_utf8(&mut t, 10);
        assert_eq!(t, "abc");
    }

    #[test]
    fn strtoul_like_c() {
        assert_eq!(strtoul("32"), Some(32));
        assert_eq!(strtoul(" \t+7px"), Some(7));
        assert_eq!(strtoul("-1"), Some(u64::MAX));
        assert_eq!(strtoul("abc"), None);
        assert_eq!(strtoul(""), None);
        assert_eq!(strtoul("-"), None);
        assert_eq!(strtoul("99999999999999999999999"), Some(u64::MAX));
    }

    #[test]
    fn strtof_like_c() {
        assert_eq!(strtof("0.55"), Some(0.55));
        assert_eq!(strtof(" -.5x"), Some(-0.5));
        assert_eq!(strtof("1."), Some(1.0));
        assert_eq!(strtof("2e2"), Some(200.0));
        assert_eq!(strtof("1e"), Some(1.0));
        assert_eq!(strtof("1e+"), Some(1.0));
        assert_eq!(strtof("."), None);
        assert_eq!(strtof("abc"), None);
        assert_eq!(strtof(""), None);
    }

    #[test]
    fn atoi_like_c() {
        assert_eq!(atoi("42abc"), 42);
        assert_eq!(atoi("  -7°"), -7);
        assert_eq!(atoi("+20"), 20);
        assert_eq!(atoi("abc"), 0);
        assert_eq!(atoi(""), 0);
        assert_eq!(atoi("99999999999999999999"), i32::MAX);
    }

    #[test]
    fn between_is_inclusive() {
        assert!(between(1, 1, 3));
        assert!(between(3, 1, 3));
        assert!(!between(4, 1, 3));
    }
}
