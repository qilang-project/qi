// Rust 调用 Qi 导出的 C ABI 函数（用显式 ASCII 符号 qi_add / qi_fib）。
extern "C" {
    fn qi_add(a: i64, b: i64) -> i64;
    fn qi_fib(n: i64) -> i64;
}
fn main() {
    unsafe {
        let s = qi_add(20, 22);
        let f = qi_fib(10);
        println!("[Rust] qi_add(20, 22) = {} (期望 42)", s);
        println!("[Rust] qi_fib(10)     = {} (期望 55)", f);
        assert_eq!(s, 42);
        assert_eq!(f, 55);
        println!("[Rust] OK");
    }
}
