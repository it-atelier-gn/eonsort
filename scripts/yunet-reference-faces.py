import json
import sys

import cv2 as cv
import numpy as np

WIDTH, HEIGHT = 96, 96
FLOOR, CEILING = 1e-4, 0.99


def main(onnx, png, out):
    y, x = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float32)
    image = np.stack(
        [
            128 + 110 * np.sin(x / 9.0) * np.cos(y / 11.0),
            128 + 110 * np.sin((x + y) / 13.0),
            128 + 110 * np.cos((x - y) / 7.0),
        ],
        axis=-1,
    )
    image = np.clip(image, 0, 255).astype(np.uint8)
    cv.imwrite(png, image)
    reread = cv.imread(png)
    assert np.array_equal(image, reread), "the png did not survive the round trip"

    net = cv.FaceDetectorYN.create(onnx, "", (WIDTH, HEIGHT), FLOOR, CEILING, 5000)
    _, faces = net.detect(reread)
    faces = [] if faces is None else faces.tolist()

    listed = [[round(value, 4) for value in row] for row in faces]
    with open(out, "w") as handle:
        json.dump(listed, handle, separators=(",", ":"))
    print(f"{len(listed)} faces -> {out}")
    print("each row is x, y, width, height, five x/y points, then the score")
    for row in listed[:5]:
        print(f"  {row[14]:.4f} {row[0]:.2f},{row[1]:.2f} {row[2]:.2f}x{row[3]:.2f}")


if __name__ == "__main__":
    main(*sys.argv[1:4])
