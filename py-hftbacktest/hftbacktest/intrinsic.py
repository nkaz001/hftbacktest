from numba.core import cgutils
from numba.core.extending import intrinsic
from numba import types


@intrinsic
def ptr_from_val(typingctx, src):
    def codegen(context, builder, signature, args):
        ptr = cgutils.alloca_once_value(builder, args[0])
        return ptr
    sig = types.CPointer(src)(src)
    return sig, codegen


@intrinsic
def val_from_ptr(typingctx, src):
    def codegen(context, builder, signature, args):
        val = builder.load(args[0])
        return val
    sig = src.dtype(src)
    return sig, codegen


@intrinsic
def address_as_void_pointer(typingctx, src):
    def codegen(context, builder, signature, args):
        return builder.inttoptr(args[0], cgutils.voidptr_t)
    sig = types.voidptr(src)
    return sig, codegen


@intrinsic
def is_null_ptr(typingctx, src):
    def codegen(context, builder, signature, args):
        return cgutils.is_null(builder, args[0])
    sig = types.boolean(src)
    return sig, codegen
