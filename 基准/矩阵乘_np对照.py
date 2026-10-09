# 张量矩阵乘.qi 的 NumPy 对照：n×n · n×n，f64 与 f32 各报平均与最快一次的 GFLOP/s。
import time
import numpy as np

for n, reps in ((64, 20000), (128, 4000), (256, 500), (512, 100), (1024, 20)):
    for dt in (np.float64, np.float32):
        rng = np.random.default_rng(1)
        a = rng.uniform(-1, 1, (n, n)).astype(dt)
        b = rng.uniform(-1, 1, (n, n)).astype(dt)
        a @ b
        best = float("inf")
        t = time.perf_counter()
        for _ in range(reps):
            t0 = time.perf_counter()
            a @ b
            best = min(best, time.perf_counter() - t0)
        avg = (time.perf_counter() - t) / reps
        flop = 2.0 * n ** 3
        print(f"n={n} {np.dtype(dt).name} 平均 {avg * 1e6:.0f} 微秒 {flop / avg / 1e9:.0f} GFLOP/s"
              f" | 最快 {best * 1e6:.0f} 微秒 {flop / best / 1e9:.0f} GFLOP/s")
