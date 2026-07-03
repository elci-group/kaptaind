mod diff {
    mod lang {
        include!("../src/diff/lang/adapter.rs");
        mod adapters {
            include!("../src/diff/lang/adapters/common.rs");
            include!("../src/diff/lang/adapters/csharp.rs");
        }
    }
}

#[test]
fn csharp_adapter_smoke() {
    use diff::lang::adapter::LanguageAdapter;
    use diff::lang::adapters::csharp::CsharpAdapter;
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.cs");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(
        b"public class Greeter { public void Hello() {} public string Name { get; set; } }",
    )
    .unwrap();

    let adapter = CsharpAdapter;
    let ast = adapter.parse_ast(&path).unwrap();
    assert!(!ast.symbols.is_empty());
    let names: Vec<_> = ast.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Greeter"));
    assert!(names.contains(&"Hello"));
    assert!(names.contains(&"Name"));
}
