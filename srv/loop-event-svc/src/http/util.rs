use paste::paste;
use rand::{RngExt, rngs::ThreadRng};

macro_rules! gckb {
    ($s:literal) => {
        paste! {
            #[inline]
            pub fn [<ckb_ $s>](k: impl std::fmt::Display) -> String {
                format!("{}:{}", $s, k)
            }
        }
    };
}

// verify code
gckb!("vc");

pub fn hex_encode(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

pub fn generate_code() -> String {
    let mut rng = ThreadRng::default();
    let n: u32 = rng.random_range(0..1000000);
    format!("{:06}", n)
}



#[test]
fn f() {
    let s = String::from("abc");
    let r = ckb_vc(&s);
    dbg!(r);
}
