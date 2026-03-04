// Copyright (c) Aptos Foundation
// Licensed pursuant to the Innovation-Enabling Source Code License, available at https://github.com/aptos-labs/aptos-core/blob/main/LICENSE

// Copyright (c) Aptos Foundation
// All Aptos Foundation code and content is licensed pursuant to the Innovation-Enabling Source Code License, available at https://github.com/aptos-labs/aptos-core/blob/main/LICENSE

//! This module validates functions with the [ConstantAccessor] attribute.
//!
//! For each non-private constant, the Move compiler generates an accessor function
//! named `const$<NAME>` that carries the [ConstantAccessor] attribute. External
//! callers access the constant's value through this function.
//!
//! ## Validation rules
//!
//! ### Phase 1: Name/Attribute correspondence (bidirectional)
//! - A function whose name starts with `const$` MUST carry [ConstantAccessor].
//! - A function carrying [ConstantAccessor] MUST have a name starting with `const$`.
//!
//! ### Phase 2: Implementation invariants
//! - No parameters (constants are values, not references).
//! - Exactly one return value whose type matches the load instruction.
//! - Body must be exactly 2 instructions: a type-matched load + `Ret`.
//!   - Primitive types (`bool`, `u8`…`u256`, `i8`…`i256`): must use the corresponding typed push
//!     instruction (`LdTrue`/`LdFalse`, `LdU8`…`LdU256`, `LdI8`…`LdI256`).
//!   - `address` and `vector<_>`: must use `LdConst(<idx>)` with a pool entry whose type matches.

use move_binary_format::{
    access::ModuleAccess,
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        Bytecode, CompiledModule, FunctionAttribute, FunctionDefinition, SignatureToken,
    },
};
use move_core_types::{
    language_storage::{CONST, DOLLAR_SIGN_DELIMITER},
    vm_status::StatusCode,
};

/// Returns `true` when the function name matches the `const$<NAME>` pattern.
fn is_const_accessor_name(function_name: &str) -> bool {
    // Must be "const$<at-least-one-char>"
    function_name
        .strip_prefix(CONST)
        .and_then(|rest| rest.strip_prefix(DOLLAR_SIGN_DELIMITER))
        .map(|rest| !rest.is_empty())
        .unwrap_or(false)
}

/// Check well-formedness of a [ConstantAccessor] attribute.
///
/// Phase 1 enforces the bidirectional name ↔ attribute correspondence.
/// Phase 2 validates signature and bytecode body.
pub fn check_const_accessor_impl(
    module: &CompiledModule,
    function_definition: &FunctionDefinition,
) -> PartialVMResult<()> {
    let handle = module.function_handle_at(function_definition.function);
    let function_name = module.identifier_at(handle.name).as_str();

    let has_const_name = is_const_accessor_name(function_name);
    let attr_count = handle
        .attributes
        .iter()
        .filter(|a| matches!(a, FunctionAttribute::ConstantAccessor))
        .count();
    if attr_count > 1 {
        return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
            .with_message("function has multiple #[const] attributes; at most one is allowed"));
    }
    let has_const_attr = attr_count == 1;

    // Phase 1: bidirectional correspondence
    match (has_const_name, has_const_attr) {
        (true, false) => {
            return Err(
                PartialVMError::new(StatusCode::INVALID_CONST_API_CODE).with_message(
                    "function name matches `const$` pattern but is missing the #[const] attribute",
                ),
            );
        },
        (false, true) => {
            return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE).with_message(
                "function has #[const] attribute but its name does not match the `const$<NAME>` pattern",
            ));
        },
        (false, false) => return Ok(()), // Regular function – nothing to check.
        (true, true) => {},              // Proceed to Phase 2.
    }

    // Phase 2: implementation invariants.

    // Must have a code body.
    let code = match &function_definition.code {
        Some(c) => c,
        None => {
            return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
                .with_message("const accessor function must have a code body (cannot be native)"));
        },
    };

    // No type parameters.
    if !handle.type_parameters.is_empty() {
        return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
            .with_message("const accessor function must have no type parameters"));
    }

    // No parameters.
    let params = module.signature_at(handle.parameters);
    if !params.0.is_empty() {
        return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
            .with_message("const accessor function must have no parameters"));
    }

    // Exactly one return value.
    let returns = module.signature_at(handle.return_);
    if returns.0.len() != 1 {
        return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
            .with_message("const accessor function must return exactly one value"));
    }
    let declared_return_type = &returns.0[0];

    // No locals (the body is a single load + Ret; locals would be dead).
    let locals = module.signature_at(code.locals);
    if !locals.0.is_empty() {
        return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
            .with_message("const accessor function must have no locals"));
    }

    validate_const_accessor_body(module, &code.code, declared_return_type)
}

/// Validate that the body of a constant accessor is a single load instruction followed by `Ret`,
/// where the load instruction matches the declared return type:
///
/// | Return type          | Required first instruction          |
/// |----------------------|-------------------------------------|
/// | `bool`               | `LdTrue` or `LdFalse`               |
/// | `u8`                 | `LdU8(_)`                           |
/// | `u16`                | `LdU16(_)`                          |
/// | `u32`                | `LdU32(_)`                          |
/// | `u64`                | `LdU64(_)`                          |
/// | `u128`               | `LdU128(_)`                         |
/// | `u256`               | `LdU256(_)`                         |
/// | `i8`                 | `LdI8(_)`                           |
/// | `i16`                | `LdI16(_)`                          |
/// | `i32`                | `LdI32(_)`                          |
/// | `i64`                | `LdI64(_)`                          |
/// | `i128`               | `LdI128(_)`                         |
/// | `i256`               | `LdI256(_)`                         |
/// | `address`, `vector`  | `LdConst(_)` with matching type     |
///
/// Primitive types must use the exact typed push instruction — `LdConst` is not accepted for them.
/// This mirrors what the Move compiler generates and keeps the verifier rule unambiguous.
fn validate_const_accessor_body(
    module: &CompiledModule,
    instrs: &[Bytecode],
    declared_return_type: &SignatureToken,
) -> PartialVMResult<()> {
    if instrs.len() != 2 {
        return Err(
            PartialVMError::new(StatusCode::INVALID_CONST_API_CODE).with_message(
                "const accessor function body must contain exactly 2 instructions: a load and Ret",
            ),
        );
    }

    // Last instruction must be Ret.
    if !matches!(instrs[1], Bytecode::Ret) {
        return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
            .with_message("const accessor function body must end with Ret"));
    }

    let load_instr = &instrs[0];

    match declared_return_type {
        // Primitive types: require the exact typed push instruction.
        SignatureToken::Bool => {
            if !matches!(load_instr, Bytecode::LdTrue | Bytecode::LdFalse) {
                return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
                    .with_message("const accessor for bool must begin with LdTrue or LdFalse"));
            }
        },
        SignatureToken::U8 => {
            if !matches!(load_instr, Bytecode::LdU8(_)) {
                return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
                    .with_message("const accessor for u8 must begin with LdU8"));
            }
        },
        SignatureToken::U16 => {
            if !matches!(load_instr, Bytecode::LdU16(_)) {
                return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
                    .with_message("const accessor for u16 must begin with LdU16"));
            }
        },
        SignatureToken::U32 => {
            if !matches!(load_instr, Bytecode::LdU32(_)) {
                return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
                    .with_message("const accessor for u32 must begin with LdU32"));
            }
        },
        SignatureToken::U64 => {
            if !matches!(load_instr, Bytecode::LdU64(_)) {
                return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
                    .with_message("const accessor for u64 must begin with LdU64"));
            }
        },
        SignatureToken::U128 => {
            if !matches!(load_instr, Bytecode::LdU128(_)) {
                return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
                    .with_message("const accessor for u128 must begin with LdU128"));
            }
        },
        SignatureToken::U256 => {
            if !matches!(load_instr, Bytecode::LdU256(_)) {
                return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
                    .with_message("const accessor for u256 must begin with LdU256"));
            }
        },
        SignatureToken::I8 => {
            if !matches!(load_instr, Bytecode::LdI8(_)) {
                return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
                    .with_message("const accessor for i8 must begin with LdI8"));
            }
        },
        SignatureToken::I16 => {
            if !matches!(load_instr, Bytecode::LdI16(_)) {
                return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
                    .with_message("const accessor for i16 must begin with LdI16"));
            }
        },
        SignatureToken::I32 => {
            if !matches!(load_instr, Bytecode::LdI32(_)) {
                return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
                    .with_message("const accessor for i32 must begin with LdI32"));
            }
        },
        SignatureToken::I64 => {
            if !matches!(load_instr, Bytecode::LdI64(_)) {
                return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
                    .with_message("const accessor for i64 must begin with LdI64"));
            }
        },
        SignatureToken::I128 => {
            if !matches!(load_instr, Bytecode::LdI128(_)) {
                return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
                    .with_message("const accessor for i128 must begin with LdI128"));
            }
        },
        SignatureToken::I256 => {
            if !matches!(load_instr, Bytecode::LdI256(_)) {
                return Err(PartialVMError::new(StatusCode::INVALID_CONST_API_CODE)
                    .with_message("const accessor for i256 must begin with LdI256"));
            }
        },
        // address and vector<_> must use LdConst with a matching pool type.
        SignatureToken::Address | SignatureToken::Vector(_) => {
            let const_idx = if let Bytecode::LdConst(idx) = load_instr {
                *idx
            } else {
                return Err(
                    PartialVMError::new(StatusCode::INVALID_CONST_API_CODE).with_message(format!(
                        "const accessor for {:?} must begin with LdConst",
                        declared_return_type
                    )),
                );
            };
            let const_type = &module.constant_at(const_idx).type_;
            if const_type != declared_return_type {
                return Err(
                    PartialVMError::new(StatusCode::INVALID_CONST_API_CODE).with_message(format!(
                        "const accessor return type does not match constant pool type \
                         (expected {:?}, found {:?})",
                        declared_return_type, const_type
                    )),
                );
            }
        },
        SignatureToken::Signer
        | SignatureToken::Function(_, _, _)
        | SignatureToken::Struct(_)
        | SignatureToken::StructInstantiation(_, _)
        | SignatureToken::Reference(_)
        | SignatureToken::MutableReference(_)
        | SignatureToken::TypeParameter(_) => {
            return Err(
                PartialVMError::new(StatusCode::INVALID_CONST_API_CODE).with_message(format!(
                    "const accessor has unsupported return type {:?}",
                    declared_return_type
                )),
            );
        },
    }

    Ok(())
}
