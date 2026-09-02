import sys
import numpy as np
import onnx
from onnx import numpy_helper
from safetensors.numpy import save_file

SOURCE = "https://huggingface.co/opencv/opencv_zoo/resolve/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx"


def convert(onnx_path, out_path):
    graph = onnx.load(onnx_path).graph
    held = {t.name: numpy_helper.to_array(t) for t in graph.initializer}

    out = {}
    prefix = None
    stem = False
    for node in graph.node:
        if node.op_type != "Conv":
            continue
        weight, bias = node.input[1], node.input[2]
        if weight in held and not weight.isdigit():
            named = weight[: -len(".weight")]
            prefix = named.rsplit(".", 1)[0]
        elif not stem:
            named, stem = "backbone.model0.conv1", True
        else:
            named = f"{prefix}.conv2"
        out[f"{named}.weight"] = held[weight]
        out[f"{named}.bias"] = held[bias]

    save_file(
        {k: np.ascontiguousarray(v).astype(np.float32) for k, v in out.items()},
        out_path,
    )
    total = sum(int(np.prod(v.shape)) for v in out.values())
    print(f"{len(out)} tensors, {total:,} parameters -> {out_path}")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} <yunet.onnx> <yunet.safetensors>")
        print(f"the onnx comes from {SOURCE}")
        raise SystemExit(2)
    convert(sys.argv[1], sys.argv[2])
