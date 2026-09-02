import re
import sys

import numpy as np
import onnx
from onnx import numpy_helper
from safetensors.numpy import save_file

SOURCE = "https://huggingface.co/opencv/opencv_zoo/resolve/main/models/face_recognition_sface/face_recognition_sface_2021dec.onnx"

PARTS = {
    "batchnorm_gamma": "bn.gamma",
    "batchnorm_beta": "bn.beta",
    "batchnorm_moving_mean": "bn.mean",
    "batchnorm_moving_var": "bn.var",
    "relu_gamma": "prelu",
    "conv2d_weight": "conv.weight",
}

TAIL = {
    "bn1_gamma": "tail.bn.gamma",
    "bn1_beta": "tail.bn.beta",
    "bn1_moving_mean": "tail.bn.mean",
    "bn1_moving_var": "tail.bn.var",
    "pre_fc1_weight": "tail.fc.weight",
    "pre_fc1_bias": "tail.fc.bias",
    "fc1_gamma": "tail.out.gamma",
    "fc1_beta": "tail.out.beta",
    "fc1_moving_mean": "tail.out.mean",
    "fc1_moving_var": "tail.out.var",
}


def renamed(name):
    if name in TAIL:
        return TAIL[name]
    match = re.fullmatch(r"conv_(\d+)_(dw_)?(.+)", name)
    if match is None:
        return None
    index, depthwise, part = match.groups()
    if part not in PARTS:
        return None
    if index == "1":
        return f"stem.{PARTS[part]}"
    return f"block{index}.{'dw' if depthwise else 'pw'}.{PARTS[part]}"


def convert(onnx_path, out_path):
    graph = onnx.load(onnx_path).graph
    held = {t.name: numpy_helper.to_array(t) for t in graph.initializer}

    for name in ("scalar_op1", "scalar_op2"):
        value = float(np.ravel(held[name])[0])
        print(f"{name} = {value}")

    out = {}
    for name, value in held.items():
        into = renamed(name)
        if into is None:
            continue
        value = np.ascontiguousarray(value).astype(np.float32)
        if into.endswith("prelu"):
            value = value.reshape(-1)
        out[into] = value

    save_file(out, out_path)
    total = sum(int(v.size) for v in out.values())
    print(f"{len(out)} tensors, {total:,} parameters -> {out_path}")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} <sface.onnx> <sface.safetensors>")
        print(f"the onnx comes from {SOURCE}")
        raise SystemExit(2)
    convert(sys.argv[1], sys.argv[2])
