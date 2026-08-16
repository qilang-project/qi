/* qi 的 整数 ↔ C 的 long（i64）。测试用：返回 3 倍。
   注意 Windows/MSVC 的 long 是 32 位！qi 的 整数 是 i64，所以这里用 long long，
   两边都是 64 位（unix 上 long==long long==64 位，改成 long long 无副作用）。 */
long long qi_test_triple(long long x) { return x * 3; }
