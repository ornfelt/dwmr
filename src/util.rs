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
    fn between_is_inclusive() {
        assert!(between(1, 1, 3));
        assert!(between(3, 1, 3));
        assert!(!between(4, 1, 3));
    }
}
