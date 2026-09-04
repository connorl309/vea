// Ways to look at the assembled bytes.

use crate::assemble::Object;

// 16 byte hexdump
pub fn hexdump(obj: &Object) -> String {
    let mut out = String::new();
    for (row, chunk) in obj.bytes.chunks(16).enumerate() {
        let mut hex = String::new();
        let mut ascii = String::new();
        for b in chunk {
            hex.push_str(&format!("{b:02x} "));
            ascii.push(if b.is_ascii_graphic() || *b == b' ' {
                *b as char
            } else {
                '.'
            });
        }
        out.push_str(&format!("{:04x}:  {hex:<48} |{ascii}|\n", row * 16));
    }
    out
}

// `address  label` lines, in address order.
pub fn symbols(obj: &Object) -> String {
    obj.symbols
        .iter()
        .map(|(name, addr)| format!("{addr:08x}  {name}\n"))
        .collect()
}