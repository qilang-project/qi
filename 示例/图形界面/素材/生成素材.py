#!/usr/bin/env python3
# 奇语 Qi · 精灵素材生成器
#
# 仓库里那两张 png 是**这个脚本画出来的**，不是从网上下的（没有版权来源问题，
# 也便于随时改配色/尺寸重生成）。两张都是 32x32、带 alpha、各约 1KB 以内。
#
#   小球.png —— 橙色小球 + 高光，用来演示"沿画布边缘转圈"
#   小猫.png —— 简笔小猫，**脸朝右**（这点很重要：朝右画才能靠水平镜像变成朝左，
#               也才能用"旋转 0 度 = 朝右"的约定去追鼠标）
#
# 重新生成：python3 生成素材.py
# 依赖：Pillow

from PIL import Image, ImageDraw

HERE = __file__.rsplit("/", 1)[0]
SIZE = 32
SCALE = 8  # 先在 8 倍画布上画再缩回来 = 手工抗锯齿，边缘不会有锯齿台阶


def new_canvas():
    return Image.new("RGBA", (SIZE * SCALE, SIZE * SCALE), (0, 0, 0, 0))


def save(img, name):
    small = img.resize((SIZE, SIZE), Image.LANCZOS)
    # 量化到 64 色调色板（FASTOCTREE 保留 alpha）：32x32 肉眼看不出差别，
    # 但文件从 2KB 掉到 1KB 上下 —— 进仓库的二进制越小越好。
    small = small.quantize(colors=64, method=Image.FASTOCTREE)
    path = f"{HERE}/{name}"
    small.save(path, "PNG", optimize=True)
    import os

    print(f"{name}: {os.path.getsize(path)} 字节")


def make_ball():
    img = new_canvas()
    d = ImageDraw.Draw(img)
    s = SCALE
    # 底色（暗）→ 往左上偏一点画亮色 → 高光：三层同心偏移＝最省事的球面感
    d.ellipse([1 * s, 1 * s, 31 * s, 31 * s], fill=(206, 104, 18, 255))
    d.ellipse([2 * s, 1 * s, 28 * s, 27 * s], fill=(255, 158, 46, 255))
    d.ellipse([5 * s, 4 * s, 21 * s, 20 * s], fill=(255, 190, 96, 255))
    d.ellipse([8 * s, 7 * s, 14 * s, 13 * s], fill=(255, 232, 190, 255))
    return img


def make_cat():
    img = new_canvas()
    d = ImageDraw.Draw(img)
    s = SCALE
    fur = (108, 176, 232, 255)
    dark = (62, 118, 168, 255)
    # 尾巴（在左边 —— 因为整只猫脸朝右）
    d.line([(3 * s, 22 * s), (8 * s, 24 * s), (10 * s, 19 * s)], fill=dark, width=2 * s)
    # 身子
    d.ellipse([6 * s, 13 * s, 24 * s, 28 * s], fill=fur)
    # 耳朵
    d.polygon([(16 * s, 9 * s), (18 * s, 2 * s), (22 * s, 7 * s)], fill=dark)
    d.polygon([(23 * s, 7 * s), (28 * s, 3 * s), (28 * s, 10 * s)], fill=dark)
    # 头（偏右 = 朝右）
    d.ellipse([14 * s, 5 * s, 30 * s, 20 * s], fill=fur)
    # 眼睛 + 鼻子（都在右半边，朝向一眼可辨）
    d.ellipse([21 * s, 10 * s, 23 * s, 13 * s], fill=(30, 40, 56, 255))
    d.ellipse([26 * s, 10 * s, 28 * s, 13 * s], fill=(30, 40, 56, 255))
    d.polygon([(28 * s, 15 * s), (31 * s, 14 * s), (29 * s, 17 * s)], fill=(255, 150, 170, 255))
    return img


if __name__ == "__main__":
    save(make_ball(), "小球.png")
    save(make_cat(), "小猫.png")
