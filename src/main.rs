use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{env, fs, io};

const HELP: &str = r#"dx - DXLib project manager

Usage:
  dx new <name> [--git]       Create a project
  dx git init                 Initialize a Git repository
  dx add class <Name>         Create include/<Name>.h and src/<Name>.cpp
  dx add file <path>          Copy an existing C/C++ file into the project
  dx add include <directory>  Add an include directory to CMake
  dx build                    Configure and build
  dx run                      Build and run
  dx clean                    Remove the build directory
  dx help                     Show this help
"#;

#[derive(Debug)]
struct Config {
    name: String,
    dxlib_root: Option<PathBuf>,
    build_dir: PathBuf,
    configuration: String,
}

fn main() -> ExitCode {
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("dx: {e}");
            ExitCode::FAILURE
        }
    }
}

fn execute() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("new") => {
            let name = args.next().ok_or("usage: dx new <name>")?;
            let git = match args.next().as_deref() {
                None => false,
                Some("--git") => true,
                Some(_) => return Err("usage: dx new <name> [--git]".into()),
            };
            no_more(args)?;
            new_project(&name, git)
        }
        Some("git") => match args.next().as_deref() {
            Some("init") => {
                no_more(args)?;
                git_init_current()
            }
            _ => Err("usage: dx git init".into()),
        },
        Some("add") => add_command(args),
        Some("build") => {
            no_more(args)?;
            build()
        }
        Some("run") => {
            no_more(args)?;
            run_project()
        }
        Some("clean") => {
            no_more(args)?;
            clean()
        }
        Some("help" | "--help" | "-h") | None => {
            print!("{HELP}");
            Ok(())
        }
        Some(x) => Err(format!("unknown command '{x}'\n\n{HELP}")),
    }
}

fn no_more(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    if args.next().is_none() {
        Ok(())
    } else {
        Err("unexpected argument".into())
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn valid_cpp_identifier(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn new_project(name: &str, git: bool) -> Result<(), String> {
    if !valid_name(name) {
        return Err("name must start with a letter or '_' and contain only ASCII letters, numbers, '_' or '-'".into());
    }
    let root = PathBuf::from(name);
    if root.exists() {
        return Err(format!("'{}' already exists", root.display()));
    }
    fs::create_dir_all(root.join("src")).map_err(err("create src"))?;
    fs::create_dir_all(root.join("include")).map_err(err("create include"))?;
    fs::create_dir_all(root.join("assets")).map_err(err("create assets"))?;
    put(root.join("CMakeLists.txt"), &cmake(name))?;
    put(root.join("src/main.cpp"), MAIN_CPP)?;
    put(root.join(".gitignore"), "/build/\n.vs/\n*.user\n*.suo\n")?;
    put(root.join("dx.toml"), &format!(
        "name = \"{name}\"\n\n# Overrides DXLIB_ROOT when non-empty.\ndxlib_root = \"\"\nbuild_dir = \"build\"\nconfiguration = \"Debug\"\n"
    ))?;
    if git {
        git_init(&root)?;
    }
    println!("Created '{name}'. Next: cd {name} && dx run");
    Ok(())
}

fn add_command(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let kind = args
        .next()
        .ok_or("usage: dx add <class|file|include> <value>")?;
    let value = args
        .next()
        .ok_or("usage: dx add <class|file|include> <value>")?;
    no_more(args)?;
    match kind.as_str() {
        "class" => add_class(&value),
        "file" => add_file(Path::new(&value)),
        "include" => add_include(Path::new(&value)),
        _ => Err("usage: dx add <class|file|include> <value>".into()),
    }
}

fn git_init_current() -> Result<(), String> {
    let (root, _) = locate()?;
    git_init(&root)
}

fn git_init(root: &Path) -> Result<(), String> {
    if root.join(".git").exists() {
        println!("Git repository already exists");
        return Ok(());
    }
    let status = Command::new("git")
        .arg("init")
        .current_dir(root)
        .status()
        .map_err(err("start git"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("git init failed ({status})"))
    }
}

fn class_path(name: &str) -> Result<PathBuf, String> {
    let normalized = name.replace('\\', "/");
    let path = PathBuf::from(normalized);
    if path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("class name must be a project-relative name without '..'".into());
    }
    if path.components().count() == 0
        || path.components().any(|c| {
            let s = c.as_os_str().to_string_lossy();
            !valid_cpp_identifier(&s)
        })
    {
        return Err(
            "class name may contain letters, numbers, '_', '-' and directory separators".into(),
        );
    }
    Ok(path)
}

fn add_class(name: &str) -> Result<(), String> {
    let (root, _) = locate()?;
    ensure_auto_sources(&root)?;
    let relative = class_path(name)?;
    let header = root.join("include").join(&relative).with_extension("h");
    let source = root.join("src").join(&relative).with_extension("cpp");
    if header.exists() || source.exists() {
        return Err(format!("class '{name}' already exists"));
    }
    if let Some(parent) = header.parent() {
        fs::create_dir_all(parent).map_err(err("create header directory"))?;
    }
    if let Some(parent) = source.parent() {
        fs::create_dir_all(parent).map_err(err("create source directory"))?;
    }
    let class_name = relative.file_name().unwrap().to_string_lossy();
    let include_name = relative
        .with_extension("h")
        .to_string_lossy()
        .replace('\\', "/");
    put(header, &format!("#pragma once\n\nclass {class_name} {{\npublic:\n    {class_name}();\n    ~{class_name}();\n}};\n"))?;
    put(source, &format!("#include \"{include_name}\"\n\n{class_name}::{class_name}() = default;\n{class_name}::~{class_name}() = default;\n"))?;
    println!("Created class {class_name}");
    Ok(())
}

fn add_file(source: &Path) -> Result<(), String> {
    if !source.is_file() {
        return Err(format!("file not found: {}", source.display()));
    }
    let ext = source
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let folder = match ext.as_str() {
        "c" | "cc" | "cpp" | "cxx" => "src",
        "h" | "hh" | "hpp" | "hxx" => "include",
        _ => return Err("supported extensions: .c .cc .cpp .cxx .h .hh .hpp .hxx".into()),
    };
    let (root, _) = locate()?;
    ensure_auto_sources(&root)?;
    let file_name = source.file_name().ok_or("invalid file path")?;
    let destination = root.join(folder).join(file_name);
    if destination.exists() {
        return Err(format!(
            "destination already exists: {}",
            destination.display()
        ));
    }
    fs::create_dir_all(root.join(folder)).map_err(err("create destination directory"))?;
    fs::copy(source, &destination).map_err(err("copy file"))?;
    println!("Added {}", destination.display());
    Ok(())
}

fn add_include(path: &Path) -> Result<(), String> {
    let (root, _) = locate()?;
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    if !full.is_dir() {
        return Err(format!("include directory not found: {}", full.display()));
    }
    let value = if path.is_absolute() {
        path.to_string_lossy().replace('\\', "/")
    } else {
        format!(
            "${{CMAKE_CURRENT_SOURCE_DIR}}/{}",
            path.to_string_lossy().replace('\\', "/")
        )
    };
    if value.contains(['"', ';', '\n']) {
        return Err("include path contains unsupported characters".into());
    }
    let cmake_path = root.join("CMakeLists.txt");
    let mut text = fs::read_to_string(&cmake_path).map_err(err("read CMakeLists.txt"))?;
    let line = format!("target_include_directories(${{PROJECT_NAME}} PRIVATE \"{value}\")");
    if text.contains(&line) {
        println!("Include directory is already registered");
        return Ok(());
    }
    let marker = "target_link_directories(";
    let pos = text
        .find(marker)
        .ok_or("could not find insertion point in CMakeLists.txt")?;
    text.insert_str(pos, &(line + "\n"));
    put(cmake_path, &text)?;
    println!("Added include directory {}", path.display());
    Ok(())
}

fn ensure_auto_sources(root: &Path) -> Result<(), String> {
    let path = root.join("CMakeLists.txt");
    let text = fs::read_to_string(&path).map_err(err("read CMakeLists.txt"))?;
    let old = "add_executable(${{PROJECT_NAME}} WIN32 src/main.cpp)";
    if text.contains(old) {
        let new = r#"file(GLOB_RECURSE PROJECT_SOURCES CONFIGURE_DEPENDS
  "src/*.c" "src/*.cc" "src/*.cpp" "src/*.cxx"
  "include/*.h" "include/*.hh" "include/*.hpp" "include/*.hxx")
add_executable(${{PROJECT_NAME}} WIN32 ${{PROJECT_SOURCES}})"#;
        put(path, &text.replace(old, new))?;
    }
    Ok(())
}

fn locate() -> Result<(PathBuf, Config), String> {
    let cwd = env::current_dir().map_err(err("read current directory"))?;
    let root = cwd
        .ancestors()
        .find(|p| p.join("dx.toml").is_file())
        .map(Path::to_path_buf)
        .ok_or("dx.toml not found")?;
    let text = fs::read_to_string(root.join("dx.toml")).map_err(err("read dx.toml"))?;
    Ok((root, parse(&text)?))
}

fn parse(text: &str) -> Result<Config, String> {
    let get = |key: &str| {
        text.lines().find_map(|line| {
            let line = line.split('#').next()?.trim();
            let (k, v) = line.split_once('=')?;
            (k.trim() == key).then(|| v.trim().trim_matches('"').to_owned())
        })
    };
    let name = get("name")
        .filter(|v| valid_name(v))
        .ok_or("invalid or missing name in dx.toml")?;
    let dxlib_root = get("dxlib_root")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    let build_dir = PathBuf::from(
        get("build_dir")
            .filter(|v| !v.is_empty())
            .unwrap_or("build".into()),
    );
    let configuration = get("configuration")
        .filter(|v| !v.is_empty())
        .unwrap_or("Debug".into());
    if build_dir.is_absolute() || build_dir.components().any(|c| c == Component::ParentDir) {
        return Err("build_dir must be a project-relative path without '..'".into());
    }
    Ok(Config {
        name,
        dxlib_root,
        build_dir,
        configuration,
    })
}

fn dxlib(config: &Config) -> Result<PathBuf, String> {
    let path = config
        .dxlib_root
        .clone()
        .or_else(|| env::var_os("DXLIB_ROOT").map(PathBuf::from))
        .ok_or("set DXLIB_ROOT or dxlib_root in dx.toml")?;
    if path.is_dir() {
        Ok(path)
    } else {
        Err(format!("DXLib directory not found: {}", path.display()))
    }
}

fn build() -> Result<(), String> {
    let (root, config) = locate()?;
    let build_dir = root.join(&config.build_dir);
    command(
        Command::new("cmake")
            .current_dir(&root)
            .args(["-S", "."])
            .arg("-B")
            .arg(&build_dir)
            .args(["-G", "Visual Studio 17 2022", "-A", "x64"])
            .arg(format!("-DDXLIB_ROOT={}", dxlib(&config)?.display())),
        "configure",
    )?;
    command(
        Command::new("cmake")
            .current_dir(&root)
            .arg("--build")
            .arg(&build_dir)
            .arg("--config")
            .arg(&config.configuration),
        "build",
    )
}

fn run_project() -> Result<(), String> {
    build()?;
    let (root, config) = locate()?;
    let base = root.join(&config.build_dir);
    let choices = [
        base.join(&config.configuration)
            .join(format!("{}.exe", config.name)),
        base.join(format!("{}.exe", config.name)),
    ];
    let exe = choices
        .iter()
        .find(|p| p.is_file())
        .ok_or_else(|| format!("executable not found: {}", choices[0].display()))?;
    let status = Command::new(exe)
        .current_dir(root)
        .status()
        .map_err(err("start application"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("application exited with {status}"))
    }
}

fn clean() -> Result<(), String> {
    let (root, config) = locate()?;
    let path = root.join(config.build_dir);
    if path.exists() {
        fs::remove_dir_all(&path).map_err(err("remove build directory"))?;
        println!("Removed {}", path.display());
    } else {
        println!("Already clean");
    }
    Ok(())
}

fn command(command: &mut Command, label: &str) -> Result<(), String> {
    println!("CMake {label}...");
    let status = command.status().map_err(|e| format!("start CMake: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("CMake {label} failed ({status})"))
    }
}

fn put(path: PathBuf, text: &str) -> Result<(), String> {
    fs::write(&path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

fn err(context: &'static str) -> impl FnOnce(io::Error) -> String {
    move |e| format!("{context}: {e}")
}

fn cmake(name: &str) -> String {
    format!(
        r#"cmake_minimum_required(VERSION 3.20)
project({name} LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

if(NOT DXLIB_ROOT)
  if(DEFINED ENV{{DXLIB_ROOT}})
    set(DXLIB_ROOT "$ENV{{DXLIB_ROOT}}")
  else()
    message(FATAL_ERROR "DXLIB_ROOT is not set")
  endif()
endif()
file(TO_CMAKE_PATH "${{DXLIB_ROOT}}" DXLIB_ROOT)

file(GLOB_RECURSE PROJECT_SOURCES CONFIGURE_DEPENDS
  "src/*.c" "src/*.cc" "src/*.cpp" "src/*.cxx"
  "include/*.h" "include/*.hh" "include/*.hpp" "include/*.hxx")
add_executable(${{PROJECT_NAME}} WIN32 ${{PROJECT_SOURCES}})
target_include_directories(${{PROJECT_NAME}} PRIVATE
  "${{CMAKE_CURRENT_SOURCE_DIR}}/include"
  "${{DXLIB_ROOT}}/プロジェクトに追加すべきファイル_VC用")
target_link_directories(${{PROJECT_NAME}} PRIVATE
  "${{DXLIB_ROOT}}/プロジェクトに追加すべきファイル_VC用")
target_link_libraries(${{PROJECT_NAME}} PRIVATE DxLib)
target_compile_definitions(${{PROJECT_NAME}} PRIVATE
  _CRT_SECURE_NO_WARNINGS
  _MBCS)
set_property(DIRECTORY PROPERTY VS_STARTUP_PROJECT ${{PROJECT_NAME}})
"#
    )
}

const MAIN_CPP: &str = r#"#include <DxLib.h>

int WINAPI WinMain(HINSTANCE, HINSTANCE, LPSTR, int) {
    ChangeWindowMode(TRUE);
    if (DxLib_Init() == -1) return -1;
    SetDrawScreen(DX_SCREEN_BACK);
    while (ProcessMessage() == 0) {
        ClearDrawScreen();
        DrawString(20, 20, "Hello, DxLib!", GetColor(255, 255, 255));
        ScreenFlip();
    }
    DxLib_End();
    return 0;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_override() {
        let c = parse("name=\"game\"\ndxlib_root=\"C:/DxLib\"\n").unwrap();
        assert_eq!(c.dxlib_root.unwrap(), PathBuf::from("C:/DxLib"));
        assert_eq!(c.build_dir, PathBuf::from("build"));
    }
    #[test]
    fn blocks_parent_build_path() {
        assert!(parse("name=\"game\"\nbuild_dir=\"../out\"\n").is_err());
    }
    #[test]
    fn validates_cpp_class_paths() {
        assert_eq!(
            class_path("game/Player_2").unwrap(),
            PathBuf::from("game/Player_2")
        );
        assert!(class_path("bad-name").is_err());
        assert!(class_path("../Player").is_err());
    }
}
