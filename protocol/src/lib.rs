pub mod protocol {
    include!(concat!(env!("OUT_DIR"), "/protocol.rs"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
    }
}
