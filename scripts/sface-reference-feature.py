import json
import sys

import cv2 as cv
import numpy as np

SIDE = 112


def main(onnx, png, out):
    y, x = np.mgrid[0:SIDE, 0:SIDE].astype(np.float32)
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

    net = cv.FaceRecognizerSF.create(onnx, "")
    feature = np.ravel(net.feature(reread)).astype(float)

    with open(out, "w") as handle:
        json.dump([round(v, 6) for v in feature], handle, separators=(",", ":"))
    print(f"{feature.size} values -> {out}")
    print(f"  first five: {[round(v, 4) for v in feature[:5]]}")
    print(f"  length: {np.linalg.norm(feature):.4f}")


if __name__ == "__main__":
    if len(sys.argv) != 4:
        print(f"usage: {sys.argv[0]} <sface.onnx> <probe.png> <reference.json>")
        raise SystemExit(2)
    main(*sys.argv[1:4])
