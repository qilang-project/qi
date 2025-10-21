use qi_compiler::lexer::*;

fn main() {
    let source_code = r#"
    函数 测试比较() {
        变量 x = 10;
        变量 y = 10;
        如果 x == y {
            返回 真;
        }
    }
    "#;

    let mut lexer = Lexer::new(source_code.to_string());
    let tokens = lexer.tokenize().unwrap();

    println!("Tokens:");
    for (i, token) in tokens.iter().enumerate() {
        println!("{}: {:?}", i, token);
    }
}