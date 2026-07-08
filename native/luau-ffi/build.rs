use std::env;

fn main() {
    let target = build_target::target_triple().unwrap();
    if target == "wasm32-unknown-emscripten" {
        if let Ok(em_dir) = env::var("EM_DIR") {
            fn exec_path(path: &str) -> String {
                if cfg!(windows) {
                    format!("{path}.bat")
                } else {
                    path.to_string()
                }
            }
            env::set_var(
                "EMCMAKE",
                exec_path(&format!("{em_dir}/emscripten/emcmake")),
            );
            env::set_var("EMMAKE", exec_path(&format!("{em_dir}/emscripten/emmake")));
            env::set_var("EM_CONFIG", format!("{em_dir}/.emscripten"));
        }
    }

    let dst = new_cmake_config().build_target("Luau.Compiler").build();
    println!(
        "cargo:warning=CMake configure (Luau.Compiler) completed: {}",
        dst.display()
    );

    let dst = new_cmake_config().build_target("Luau.Require").build();
    println!(
        "cargo:warning=CMake configure (Luau.Require) completed: {}",
        dst.display()
    );

    let target = build_target::target_triple().unwrap();
    if target == "aarch64-unknown-linux-gnu" {
        println!("cargo:rustc-link-search=native={}/build", dst.display());
        println!("cargo:rustc-link-lib=dylib=stdc++");
    } else if target == "x86_64-pc-windows-msvc" || target == "aarch64-pc-windows-msvc" {
        println!(
            "cargo:rustc-link-search=native={}/build/Release",
            dst.display()
        );
    } else {
        println!("cargo:rustc-link-search=native={}/build", dst.display());
        println!("cargo:rustc-link-lib=dylib=c++");
    }

    println!("cargo:rustc-link-lib=static=Luau.Ast");
    println!("cargo:rustc-link-lib=static=Luau.Config");
    println!("cargo:rustc-link-lib=static=Luau.Compiler");
    println!("cargo:rustc-link-lib=static=Luau.VM");
    println!("cargo:rustc-link-lib=static=Luau.RequireNavigator");
    println!("cargo:rustc-link-lib=static=Luau.Require");

    bindgen::Builder::default()
        .headers([
            "../../luau/VM/include/lua.h",
            "../../luau/VM/include/lualib.h",
            "../../luau/Compiler/include/luacode.h",
        ])
        .clang_arg(format!("--target={}", target))
        .clang_arg("-fvisibility=default")
        .layout_tests(false)
        .generate()
        .unwrap()
        .write_to_file("src/luau.rs")
        .unwrap();

    bindgen::Builder::default()
        .headers(["../../luau/Require/Runtime/include/Luau/Require.h"])
        .clang_arg("-x")
        .clang_arg("c++")
        .clang_arg("-std=c++17")
        .clang_arg("-I../../luau/Compiler/include")
        .clang_arg("-I../../luau/VM/include")
        .clang_arg("-I../../luau/Require/Runtime/include")
        .clang_arg("-DLUA_API=extern\"C\"")
        .allowlist_function("luarequire_.*")
        .allowlist_function("luaopen_require")
        .allowlist_type("luarequire_.*")
        .blocklist_type("lua_State")
        .raw_line("use super::luau::*;")
        .clang_arg(format!("--target={}", target))
        .clang_arg("-fvisibility=default")
        .layout_tests(false)
        .generate()
        .unwrap()
        .write_to_file("src/luau_require.rs")
        .unwrap();

    let cs = new_csbindgen_builder("src/luau.rs")
        .rust_file_header("use super::luau::*;")
        .csharp_file_header(
            "
using lua_newstate_f_delegate = Luau.Native.lua_Alloc;
using lua_pushcclosurek_fn__delegate = Luau.Native.lua_CFunction;
using lua_tocfunction_return_delegate = Luau.Native.lua_CFunction;
using lua_pushcclosurek_cont_delegate = Luau.Native.lua_Continuation;
using lua_setuserdatadtor_dtor_delegate = Luau.Native.lua_Destructor;
using lua_getuserdatadtor_return_delegate = Luau.Native.lua_Destructor;
using lua_getallocf_return_delegate = Luau.Native.lua_Alloc;
using lua_getcoverage_callback_delegate = Luau.Native.lua_Coverage;
",
        );

    cs.generate_to_file(
        "src/luau_ffi.rs",
        "../../src/Luau.Native/NativeMethods.g.cs",
    )
    .unwrap();

    cs.csharp_dll_name_if("(UNITY_IOS || UNITY_WEBGL) && !UNITY_EDITOR", "__Internal")
        .generate_csharp_file("../../src/Luau.Unity/Assets/Luau.Unity/Native/NativeMethods.g.cs")
        .unwrap();

    let cs2 = new_csbindgen_builder("src/luau_require.rs")
        .rust_file_header(
            "
use super::luau::*;
use super::luau_require::*;
",
        )
        .csharp_disable_emit_dll_name(true)
        .csharp_file_header(
            "
using luarequire_pushrequire_config_init_delegate = Luau.Native.luarequire_Configuration_init;     
using luaopen_require_config_init_delegate = Luau.Native.luarequire_Configuration_init;    
using luarequire_pushproxyrequire_config_init_delegate = Luau.Native.luarequire_Configuration_init;   
",
        );

    cs2.generate_to_file(
        "src/luau_require_ffi.rs",
        "../../src/Luau.Native/NativeMethods.Require.g.cs",
    )
    .unwrap();

    cs2.generate_csharp_file(
        "../../src/Luau.Unity/Assets/Luau.Unity/Native/NativeMethods.Require.g.cs",
    )
    .unwrap();
}

fn new_cmake_config() -> cmake::Config {
    let mut config = cmake::Config::new("../../luau");

    let target = build_target::target_triple().unwrap();

    if target == "x86_64-pc-windows-msvc" || target == "aarch64-pc-windows-msvc" {
        if let Ok(cc) = std::env::var("CC") {
            if !cc.is_empty() {
                config.define("CMAKE_C_COMPILER", cc);
            }
        }
        if let Ok(cxx) = std::env::var("CXX") {
            if !cxx.is_empty() {
                config.define("CMAKE_CXX_COMPILER", cxx);
            }
        }

        config.cxxflag("/EHsc");
    } else if target == "aarch64-unknown-linux-gnu" {
        config.define("CMAKE_SYSTEM_NAME", "Linux");
        config.define("CMAKE_SYSTEM_PROCESSOR", "aarch64");
        config.define("CMAKE_C_FLAGS", "-ffunction-sections -fdata-sections -fPIC");
        config.define(
            "CMAKE_CXX_FLAGS",
            "-ffunction-sections -fdata-sections -fPIC",
        );
        if let Ok(cc) = std::env::var("CC") {
            if !cc.is_empty() {
                config.define("CMAKE_C_COMPILER", cc);
            }
        }
        if let Ok(cxx) = std::env::var("CXX") {
            if !cxx.is_empty() {
                config.define("CMAKE_CXX_COMPILER", cxx);
            }
        }
    } else if target == "x86_64-apple-ios" {
        config.define("CMAKE_SYSTEM_NAME", "iOS");
        config.define("CMAKE_SYSTEM_PROCESSOR", "x86_64");
        config.define("CMAKE_OSX_ARCHITECTURES", "x86_64");
        config.define("CMAKE_OSX_SYSROOT", "/Applications/Xcode.app/Contents/Developer/Platforms/iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator.sdk");
        config.define(
            "CMAKE_C_FLAGS",
            "-fPIC -m64 --target=x86_64-apple-ios-simulator -mios-simulator-version-min=17.5",
        );
        config.define(
            "CMAKE_CXX_FLAGS",
            "-fPIC -m64 --target=x86_64-apple-ios-simulator -mios-simulator-version-min=17.5",
        );
    } else if target == "aarch64-apple-ios" {
        config.define("CMAKE_SYSTEM_NAME", "iOS");
        config.define("CMAKE_SYSTEM_PROCESSOR", "arm64");
        config.define("CMAKE_OSX_ARCHITECTURES", "arm64");
        config.define("CMAKE_OSX_SYSROOT", "/Applications/Xcode.app/Contents/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk");
        config.define(
            "CMAKE_C_FLAGS",
            "-fPIC --target=arm64-apple-ios -miphoneos-version-min=17.5",
        );
        config.define(
            "CMAKE_CXX_FLAGS",
            "-fPIC --target=arm64-apple-ios -miphoneos-version-min=17.5",
        );
    } else if target == "aarch64-linux-android" {
        let ndk_home = std::env::var("ANDROID_NDK_HOME").unwrap();
        let ndk_bin = format!("{}/toolchains/llvm/prebuilt/linux-x86_64/bin", ndk_home);
        config.define("CMAKE_SYSTEM_NAME", "Android");
        config.define("CMAKE_SYSTEM_PROCESSOR", "aarch64");
        config.define("CMAKE_ANDROID_ARCH_ABI", "arm64-v8a");
        config.define("CMAKE_ANDROID_NDK", &ndk_home);
        config.define("CMAKE_ANDROID_STL_TYPE", "c++_static");
        config.define("CMAKE_ANDROID_API", "26");
        config.define(
            "CMAKE_C_COMPILER",
            format!("{}/aarch64-linux-android26-clang", ndk_bin),
        );
        config.define(
            "CMAKE_CXX_COMPILER",
            format!("{}/aarch64-linux-android26-clang++", ndk_bin),
        );
        config.define(
            "CMAKE_C_FLAGS",
            "-DANDROID -ffunction-sections -fdata-sections -fPIC",
        );
        config.define(
            "CMAKE_CXX_FLAGS",
            "-DANDROID -ffunction-sections -fdata-sections -fPIC",
        );
    } else if target == "x86_64-linux-android" {
        let ndk_home = std::env::var("ANDROID_NDK_HOME").unwrap();
        let ndk_bin = format!("{}/toolchains/llvm/prebuilt/linux-x86_64/bin", ndk_home);
        config.define("CMAKE_SYSTEM_NAME", "Android");
        config.define("CMAKE_SYSTEM_PROCESSOR", "x86_64");
        config.define("CMAKE_ANDROID_ARCH_ABI", "x86_64");
        config.define("CMAKE_ANDROID_NDK", &ndk_home);
        config.define("CMAKE_ANDROID_STL_TYPE", "c++_static");
        config.define("CMAKE_ANDROID_API", "26");
        config.define(
            "CMAKE_C_COMPILER",
            format!("{}/x86_64-linux-android26-clang", ndk_bin),
        );
        config.define(
            "CMAKE_CXX_COMPILER",
            format!("{}/x86_64-linux-android26-clang++", ndk_bin),
        );
        config.define(
            "CMAKE_C_FLAGS",
            "-DANDROID -ffunction-sections -fdata-sections -fPIC -m64",
        );
        config.define(
            "CMAKE_CXX_FLAGS",
            "-DANDROID -ffunction-sections -fdata-sections -fPIC -m64",
        );
    } else if target == "wasm32-unknown-emscripten" {
        if let Ok(em_dir) = env::var("EM_DIR") {
            // By default cmake crate overrides compiler paths with not qualified ones, causing missing compiler errors with no emscripten in PATH.
            config.define(
                "CMAKE_C_COMPILER",
                format!("{}/emscripten/emcc", em_dir),
            );
            config.define(
                "CMAKE_CXX_COMPILER",
                format!("{}/emscripten/em++", em_dir),
            );
        }
        config.define("CMAKE_C_FLAGS", "-ffunction-sections -fdata-sections -fPIC");
        config.define(
            "CMAKE_CXX_FLAGS",
            "-ffunction-sections -fdata-sections -fPIC",
        );
    }

    config
}

fn new_csbindgen_builder(src: &'static str) -> csbindgen::Builder {
    csbindgen::Builder::default()
        .input_bindgen_file(src)
        .rust_method_prefix("ffi_")
        .csharp_entry_point_prefix("ffi_")
        .csharp_method_prefix("")
        .csharp_namespace("Luau.Native")
        .csharp_dll_name("libluau")
        .csharp_class_accessibility("public")
        .csharp_generate_const_filter(|x| x.starts_with("LUA"))
        .csharp_use_function_pointer(false)
}
