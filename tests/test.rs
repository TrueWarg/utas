use assert_cmd::Command;
use assert_fs::{self};
use file::{CompareDirsContentResult, Diff, DirDiff};
use std::{error::Error, path::Path};
use std::fs::create_dir;

#[test]
fn case_android_1() -> Result<(), Box<dyn Error>> {
    basic_test_case("case1", None)
}

#[test]
fn case_android_2() -> Result<(), Box<dyn Error>> {
    basic_test_case("case2", None)
}

#[test]
fn case_android_3() -> Result<(), Box<dyn Error>> {
    basic_test_case("case3", None)
}

#[test]
fn case_android_4() -> Result<(), Box<dyn Error>> {
    basic_test_case("case4", None)
}

#[test]
fn case_android_5() -> Result<(), Box<dyn Error>> {
    basic_test_case("case5", None)
}

#[test]
fn case_android_6() -> Result<(), Box<dyn Error>> {
    basic_test_case("case6", None)
}

#[test]
fn case_android_7() -> Result<(), Box<dyn Error>> {
    basic_test_case("case7", None)
}

#[test]
fn case_android_8() -> Result<(), Box<dyn Error>> {
    basic_test_case("case8", None)
}

#[test]
fn case_android_9() -> Result<(), Box<dyn Error>> {
    basic_test_case("case9", None)
}

#[test]
fn case_android_10() -> Result<(), Box<dyn Error>> {
    basic_test_case("case10", Some("ru".to_string()))
}

#[test]
fn case_android_11() -> Result<(), Box<dyn Error>> {
    basic_test_case("case11", Some("mn".to_string()))
}

fn basic_test_case(case_rel_path: &str, defailt_lang: Option<String>) -> Result<(), Box<dyn Error>> {
    let mut cmd = Command::cargo_bin("utas")?;

    let mut temp = assert_fs::TempDir::new()?;
    temp = temp.into_persistent();

    let input = Path::new("tests").join("cases").join("android").join(case_rel_path).join("input");
    let output = temp.path();
    let expected = Path::new("tests").join("cases").join("android").join(case_rel_path).join("output");

    cmd.arg(Path::new(&input).as_os_str()).arg(output.as_os_str());
    if defailt_lang.is_some() {
        cmd.arg(defailt_lang.unwrap());
    }
    cmd.assert().success();
    let result = file::compare_dirs_content(expected, output)?;
    match &result {
        CompareDirsContentResult::Eq => (),
        CompareDirsContentResult::Diffs(diffs) => eprintln!("{}", format_diffs(diffs)),
    }
    assert!(
        CompareDirsContentResult::Eq == result,
        "Dirs are different. See log above"
    );
    Ok(())
}

fn format_result(result: CompareDirsContentResult) -> String {
    match result {
        CompareDirsContentResult::Eq => "OK".to_string(),
        CompareDirsContentResult::Diffs(diffs) => format_diffs(&diffs),
    }
}

fn format_diffs(diffs: &Vec<DirDiff>) -> String {
    let mut result = "".to_string();
    let mut index = 0;
    for diff in diffs {
        let item = match diff {
            DirDiff::Path { left, right } => format!(
                "{}. Paths are different: {} and {}\n___________________________________________________________\n\n",
                index,
                format_path(left),
                format_path(right)
            ),
            DirDiff::FileContent { path, diffs } => {
                format!(
                    "{}. In file {} diff content:\n {}___________________________________________________________\n\n",
                    index,
                    path,
                    format_file_diffs(diffs),
                )
            }
        };
        index += 1;
        result.push_str(&item);
    }
    result
}

fn format_path(path: &Option<String>) -> String {
    match path {
        Some(path) => path.clone(),
        None => "|NO ANALOGUE|".to_string(),
    }
}

fn format_file_diffs(diffs: &Vec<Diff>) -> String {
    let mut result = "".to_string();
    for diff in diffs {
        result.push_str(&format!(
            "  Line {}.:\n{}\n{}",
            diff.line_number, diff.left, diff.right
        ));
    }

    result
}
