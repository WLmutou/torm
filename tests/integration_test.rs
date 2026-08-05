#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!("0.2.0", env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_hello() {
        assert_eq!("Hello, world!", "Hello, world!");
    }
}