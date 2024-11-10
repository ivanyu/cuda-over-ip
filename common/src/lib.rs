#[macro_use]
extern crate num_derive;

#[allow(non_camel_case_types)]
#[repr(i32)]
#[derive(PartialEq, Debug, FromPrimitive)]
pub enum RPC {
    cuDriverGetVersion = 1
}

impl RPC {
    pub fn parse(value: i32) -> Self {
        use num_traits::FromPrimitive;
        FromPrimitive::from_i32(value).expect(&format!("Invalid RPC value: {}", value))
    }
}

#[cfg(test)]
mod tests {
    use crate::RPC;

    #[test]
    fn conversion() {
        let original = RPC::cuDriverGetVersion;
        let v: i32 = RPC::cuDriverGetVersion as i32;
        let rpc = RPC::parse(v);
        assert_eq!(rpc, original);
    }
}
