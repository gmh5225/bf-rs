use std::ffi::{CString, CStr};
use std::os::raw::c_char;
use std::ptr;
use std::cell::RefCell;

use llvm_sys::prelude::*;
use llvm_sys::core::*;
use llvm_sys::analysis::{LLVMVerifyModule, LLVMVerifierFailureAction};
use llvm_sys::transforms::pass_manager_builder as builder;
use llvm_sys::execution_engine as engine;

// FIXME: Force to link against libffi
#[link(name = "ffi")]
extern {}

pub struct Context {
    context_ref: LLVMContextRef,
    strings:     RefCell<Vec<CString>>,
}

impl Context {
    pub fn new() -> Self {
        Context {
            context_ref: unsafe { LLVMContextCreate() },
            strings:     RefCell::new(Vec::new()),
        }
    }

    pub fn new_name(&self, name: &str) -> *const c_char {
        let string = CString::new(name).unwrap();
        let ptr    = string.as_ptr();
        self.strings.borrow_mut().push(string);
        ptr
    }

    fn wrap_value(&self, value_ref: LLVMValueRef) -> Value {
        Value {
            value_ref: value_ref,
            context:   self,
        }
    }

    fn wrap_type(&self, type_ref: LLVMTypeRef) -> Type {
        Type {
            type_ref: type_ref,
            context:  self,
        }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            LLVMContextDispose(self.context_ref);
        }
    }
}

pub struct Module<'a> {
    module_ref: LLVMModuleRef,
    context:    &'a Context,
}

impl<'a> Module<'a> {
    pub fn new(context: &'a Context, name: &str) -> Self {
        let name = context.new_name(name);
        Module {
            module_ref: unsafe {
                LLVMModuleCreateWithNameInContext(name, context.context_ref)
            },
            context: context,
        }
    }

    pub fn add_function(&self, name: &str, ty: Type<'a>) -> Value<'a> {
        let name = self.context.new_name(name);
        self.context.wrap_value(unsafe {
            LLVMAddFunction(self.module_ref, name, ty.type_ref)
        })
    }

    // From llvm-alt:
    pub fn optimize(&self, opt_level: usize, size_level: usize) {
        unsafe {
            let builder = builder::LLVMPassManagerBuilderCreate();
            builder::LLVMPassManagerBuilderSetOptLevel(builder, opt_level as _);
            builder::LLVMPassManagerBuilderSetSizeLevel(builder, size_level as _);
            let pass_manager = LLVMCreatePassManager();
            builder::LLVMPassManagerBuilderPopulateModulePassManager(builder, pass_manager);
            builder::LLVMPassManagerBuilderDispose(builder);
            LLVMRunPassManager(pass_manager, self.module_ref);
            LLVMDisposePassManager(pass_manager);
        }
    }

    pub fn dump(&self) {
        unsafe {
            LLVMDumpModule(self.module_ref);
        }
    }

    pub fn verify(&self) -> Result<(), String> {
        let mut out_message: *mut c_char = ptr::null_mut();

        unsafe {
            if LLVMVerifyModule(self.module_ref,
                                LLVMVerifierFailureAction::LLVMReturnStatusAction,
                                &mut out_message) == 0 {
                Ok(())
            } else {
                let result = CStr::from_ptr(out_message).to_string_lossy().into_owned();
                LLVMDisposeMessage(out_message);
                Err(result)
            }
        }
    }

    pub fn run_function(&self, fun: Value<'a>) -> Result<usize, String> {
        let mut out_message: *mut c_char = ptr::null_mut();
        let mut exec: engine::LLVMExecutionEngineRef = ptr::null_mut();

        unsafe {
            engine::LLVMLinkInInterpreter();
            engine::LLVMLinkInMCJIT();

            if engine::LLVMCreateExecutionEngineForModule(&mut exec, self.module_ref, &mut out_message) != 0 {
                let result = CStr::from_ptr(out_message).to_string_lossy().into_owned();
                LLVMDisposeMessage(out_message);
                return Err(result);
            }

            let size = engine::LLVMCreateGenericValueOfInt(Type::get_i64(self.context).type_ref,
                                                           30_000 as _,
                                                           0 as i32);
//            let read = engine::LLVMCreateGenericValueOfPointer(rts_read as _);
//            let write = engine::LLVMCreateGenericValueOfPointer(rts_write as _);
            let mut args = vec![size];
            let result = engine::LLVMRunFunction(exec,
                                                 fun.value_ref,
                                                 args.len() as u32,
                                                 args.as_mut_ptr());
            Ok(engine::LLVMGenericValueToInt(result, 0 as i32) as _)
        }
    }
}

#[derive(Copy, Clone)]
pub struct Type<'a> {
    type_ref:  LLVMTypeRef,
    context:   &'a Context,
}

impl<'a> Type<'a> {
    pub fn get_i64(context: &'a Context) -> Self {
        context.wrap_type(unsafe {
            LLVMInt64TypeInContext(context.context_ref)
        })
    }

    pub fn get_i32(context: &'a Context) -> Self {
        context.wrap_type(unsafe {
            LLVMInt32TypeInContext(context.context_ref)
        })
    }

    pub fn get_i8(context: &'a Context) -> Self {
        context.wrap_type(unsafe {
            LLVMInt8TypeInContext(context.context_ref)
        })
    }

    pub fn get_bool(context: &'a Context) -> Self {
        context.wrap_type(unsafe {
            LLVMInt1TypeInContext(context.context_ref)
        })
    }

    pub fn get_void(context: &'a Context) -> Self {
        context.wrap_type(unsafe {
            LLVMVoidTypeInContext(context.context_ref)
        })
    }

    pub fn get_pointer(target: Type<'a>) -> Self {
        target.context.wrap_type(unsafe {
            LLVMPointerType(target.type_ref, 0)
        })
    }

    pub fn get_function(args: &[Type<'a>], result: Type<'a>) -> Self {
        let mut args = args.into_iter().map(|arg| arg.type_ref).collect::<Vec<_>>();
        result.context.wrap_type(unsafe {
            LLVMFunctionType(result.type_ref,
                             args.as_mut_ptr(),
                             args.len() as _,
                             0)
        })
    }
}

#[derive(Copy, Clone)]
pub struct Value<'a> {
    value_ref: LLVMValueRef,
    context:   &'a Context,
}

impl<'a> Value<'a> {
    pub fn get_fun_param(&self, index: usize) -> Self {
        self.context.wrap_value(unsafe {
            LLVMGetParam(self.value_ref, index as _)
        })
    }

    pub fn append(&self, name: &str) -> BasicBlock<'a> {
        let name = self.context.new_name(name);
        let bb_ref = unsafe {
            LLVMAppendBasicBlockInContext(self.context.context_ref,
                                          self.value_ref,
                                          name)
        };
        BasicBlock {
            bb_ref: bb_ref,
            _context: self.context,
        }
    }

    pub fn get_u64(context: &'a Context, value: u64) -> Self {
        context.wrap_value(unsafe {
            LLVMConstInt(Type::get_i64(context).type_ref,
                         value as _,
                         false as _)
        })
    }

    pub fn get_u32(context: &'a Context, value: u32) -> Self {
        context.wrap_value(unsafe {
            LLVMConstInt(Type::get_i32(context).type_ref,
                         value as _,
                         false as _)
        })
    }

    pub fn get_u8(context: &'a Context, value: u8) -> Self {
        context.wrap_value(unsafe {
            LLVMConstInt(Type::get_i8(context).type_ref,
                         value as _,
                         false as _)
        })
    }


    pub fn get_bool(context: &'a Context, value: bool) -> Self {
        context.wrap_value(unsafe {
            LLVMConstInt(Type::get_bool(context).type_ref,
                         value as _,
                         false as _)
        })
    }
}

#[derive(Copy, Clone)]
pub struct BasicBlock<'a> {
    bb_ref:  LLVMBasicBlockRef,
    _context: &'a Context,
}

pub struct Builder<'a> {
    builder_ref: LLVMBuilderRef,
    context:     &'a Context,
}

impl<'a> Builder<'a> {
    pub fn new(context: &'a Context) -> Self {
        Builder {
            builder_ref: unsafe { LLVMCreateBuilderInContext(context.context_ref) },
            context:     context,
        }
    }

    pub fn position_at_end(&self, bb: BasicBlock<'a>) {
        unsafe {
            LLVMPositionBuilderAtEnd(self.builder_ref, bb.bb_ref);
        }
    }

    pub fn add(&self, v1: Value<'a>, v2: Value<'a>, name: &str) -> Value<'a> {
        let name = self.context.new_name(name);
        self.context.wrap_value(unsafe {
            LLVMBuildAdd(self.builder_ref, v1.value_ref, v2.value_ref, name)
        })
    }

    pub fn alloca(&self, ty: Type<'a>, name: &str) -> Value<'a> {
        let name = self.context.new_name(name);
        self.context.wrap_value(unsafe {
            LLVMBuildAlloca(self.builder_ref, ty.type_ref, name)
        })
    }

    pub fn array_alloca(&self, ty: Type<'a>, size: Value<'a>, name: &str) -> Value<'a> {
        let name = self.context.new_name(name);
        self.context.wrap_value(unsafe {
            LLVMBuildArrayAlloca(self.builder_ref,
                                 ty.type_ref,
                                 size.value_ref,
                                 name)
        })
    }

    pub fn call(&self, fun: Value<'a>, args: &[Value<'a>], name: &str) -> Value<'a> {
        let name = self.context.new_name(name);
        let mut args = args.into_iter().map(|arg| arg.value_ref).collect::<Vec<_>>();
        self.context.wrap_value(unsafe {
            LLVMBuildCall(self.builder_ref,
                          fun.value_ref,
                          args.as_mut_ptr(),
                          args.len() as u32,
                          name)
        })
    }

    pub fn gep(&self, ptr: Value<'a>, indices: &[Value<'a>], name: &str) -> Value<'a> {
        let name = self.context.new_name(name);
        let mut indices = indices.into_iter().map(|i| i.value_ref).collect::<Vec<_>>();
        self.context.wrap_value(unsafe {
            LLVMBuildGEP(self.builder_ref,
                         ptr.value_ref,
                         indices.as_mut_ptr(),
                         indices.len() as u32,
                         name)
        })
    }

    pub fn load(&self, ptr: Value<'a>, name: &str) -> Value<'a> {
        let name = self.context.new_name(name);
        self.context.wrap_value(unsafe {
            LLVMBuildLoad(self.builder_ref, ptr.value_ref, name)
        })
    }

    pub fn ret(&self, value: Value<'a>) {
        unsafe {
            LLVMBuildRet(self.builder_ref, value.value_ref);
        }
    }

    pub fn store(&self, src: Value<'a>, dst: Value<'a>) {
        unsafe {
            LLVMBuildStore(self.builder_ref, src.value_ref, dst.value_ref);
        }
    }

    pub fn sub(&self, v1: Value<'a>, v2: Value<'a>, name: &str) -> Value<'a> {
        let name = self.context.new_name(name);
        self.context.wrap_value(unsafe {
            LLVMBuildSub(self.builder_ref, v1.value_ref, v2.value_ref, name)
        })
    }

    pub fn trunc(&self, value: Value<'a>, ty: Type<'a>, name: &str) -> Value<'a> {
        let name = self.context.new_name(name);
        self.context.wrap_value(unsafe {
            LLVMBuildTrunc(self.builder_ref, value.value_ref, ty.type_ref, name)
        })
    }

    pub fn zext(&self, value: Value<'a>, ty: Type<'a>, name: &str) -> Value<'a> {
        let name = self.context.new_name(name);
        self.context.wrap_value(unsafe {
            LLVMBuildZExt(self.builder_ref, value.value_ref, ty.type_ref, name)
        })
    }
}
