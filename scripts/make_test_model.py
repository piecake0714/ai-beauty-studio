"""Write models/files/test-luma-v1.onnx, the AI runtime's self-test model.

The model computes Rec. 709 luma from an RGB image with one 1x1
convolution: input "image" float32 [1, 3, H, W] (values 0..1), output
"luma" float32 [1, 1, H, W]. It is created here from scratch (no
third-party weights) so it can ship without licensing questions.

It is written with a minimal protobuf encoder so no ONNX tooling is needed.
Run: python scripts/make_test_model.py
"""
import os
import struct


def varint(n):
    out = bytearray()
    while True:
        byte = n & 0x7F
        n >>= 7
        if n:
            out.append(byte | 0x80)
        else:
            out.append(byte)
            return bytes(out)


def key(field, wire):
    return varint((field << 3) | wire)


def f_int(field, n):
    return key(field, 0) + varint(n)


def f_bytes(field, data):
    if isinstance(data, str):
        data = data.encode("utf-8")
    return key(field, 2) + varint(len(data)) + data


FLOAT = 1  # TensorProto.DataType.FLOAT


def tensor(name, dims, values):
    return (b"".join(f_int(1, d) for d in dims) + f_int(2, FLOAT) + f_bytes(8, name)
            + f_bytes(9, struct.pack("<%df" % len(values), *values)))


def dim(value):
    return f_bytes(1, f_int(1, value) if isinstance(value, int) else f_bytes(2, value))


def value_info(name, dims):
    shape = b"".join(f_bytes(1, dim(d)) for d in dims)
    tensor_type = f_int(1, FLOAT) + f_bytes(2, shape)
    return f_bytes(1, name) + f_bytes(2, f_bytes(1, tensor_type))


def node(inputs, outputs, name, op):
    return (b"".join(f_bytes(1, i) for i in inputs) + b"".join(f_bytes(2, o) for o in outputs)
            + f_bytes(3, name) + f_bytes(4, op))


def main():
    weights = tensor("weights", [1, 3, 1, 1], [0.2126, 0.7152, 0.0722])
    bias = tensor("bias", [1], [0.0])
    graph = (f_bytes(1, node(["image", "weights", "bias"], ["luma"], "luma_conv", "Conv"))
             + f_bytes(2, "test_luma")
             + f_bytes(5, weights) + f_bytes(5, bias)
             + f_bytes(11, value_info("image", [1, 3, "height", "width"]))
             + f_bytes(12, value_info("luma", [1, 1, "height", "width"])))
    model = (f_int(1, 8)                         # ir_version
             + f_bytes(2, "ai-beauty-studio")      # producer_name
             + f_bytes(7, graph)
             + f_bytes(8, f_bytes(1, "") + f_int(2, 13)))  # opset_import: default domain, opset 13
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    path = os.path.join(root, "models", "files", "test-luma-v1.onnx")
    with open(path, "wb") as f:
        f.write(model)
    print(f"wrote {path} ({len(model)} bytes)")


if __name__ == "__main__":
    main()
