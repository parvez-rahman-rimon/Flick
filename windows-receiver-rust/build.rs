use std::env;
use std::path::Path;

fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("flick.ico");
        let _ = res.compile();

        if let Ok(out_dir) = env::var("OUT_DIR") {
            let res_o = Path::new(&out_dir).join("resource.o");
            if res_o.exists() {
                // For GNU MinGW toolchain, pass object file directly to linker
                // so GNU ld does not drop the .rsrc section from static archive
                println!("cargo:rustc-link-arg={}", res_o.display());
            }
        }
    }
}
