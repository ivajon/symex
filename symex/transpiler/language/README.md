<h1 align="center">
  Symex General Assembly Intermediate Language
</h1>

This repository defines a small DSL used to define [symex](github.com/s7rul/symex) general_assembly instructions more cleanly and shortly rather than writing out struct by hand. This crate only makes sense in the context of Symex as of now.

# Language reference

<details><summary> preface </summary>
This language reference denotes optional things with the following syntax

```rust
{name}
```

where `name` is the optional name or identifier.
Moreover, it denotes a position where there are multiple valid symbols as

```rust
<operation>
```

where `operation` can be any of the valid operations
</details>

The language is built to be as close to ordinary rust as possible while remaining minimal.
It only defines a few constructs, these are as follows:

## Top-level elements

### Assignment

Assigns the value stored in the `operand` to the `destination`.

```rust
{let} destination = operand;
```

### Binary operations

The language supports all of the binary operations defined in [Symex](github.com/s7rul/symex) general assembly and denotes them as

```rust
{let} destination = operand1 <operation> operand2
```

or

```rust
destination <operation> = operand2; // Equivalent to destination = destination <operation> operand2
```

<details><summary> where `operation` can be any of the following </summary>

| Syntax |      Description       |
| :----: | :--------------------: |
|  `+`   |        Addition        |
| `adc`  |     Add with carry     |
|  `-`   |      Subtraction       |
|  `*`   |     Multiplication     |
|  `/`   |   Unsigned division    |
|  `^`   |       Binary xor       |
|  `\|`  |       Binary or        |
|  `&`   |       Binary and       |
|  `<<`  |   Logical left shift   |
|  `>>`  |  Logical right shift   |
| `asr`  | Arithmetic right shift |
| `ror`  |      Rotate right      |
</details>

### Unary operations

The language only supports one unary operation `!` which can be utilized using the following syntax.

```rust
{let} destination = !operand;
```

### Control-flow

The language supports two different control flows, the `if` statement and the `for` loop. The `for` loop follows normal rust syntax
i.e.

```rust
for variable in iterator {
    ...
}
```

while the `if` statement modifies it slightly to require parenthesis around the condition i.e.

```rust
if (condition) {
    ...
}
```

both of these examples are evaluated as normal rust code while the code inside of the blocks is interpreted as pseudo-code.

### Function calls

The language defines two types of functions, the first is compiler intrinsic operations, which are defined by the language and follow the case-specific syntax. But in general, they follow this syntax

```rust
<function_name>(<first_operand>,{other operands})
```

where the number of arguments varies depending on what function is being called.
The other type of function calls the transpiler supports is a backup where it will interpret the above syntax as a call to an external rust function which then will be invoked as an [operand](#operands).
It is important to note that intrinsics parsing is **case-insensitive**.

<details><summary> listing of intrinsics </summary>
The defined intrinsics are as follows

<details><summary> ZeroExtend </summary>
Zero extends the [operand](#operands) using the specified bit index as the last bit, discarding all bits after that one.

```rust
{let} destination = ZeroExtend(operand,<last bit>);
```

</details>

<details><summary> SignExtend </summary>
Sign extends the [operand](#operands) using the specified bit index as the sign bit.

```rust
{let} destination = SignExtend(operand,<sign bit>);
```

</details>

<details><summary> Resize </summary>
Resizes the [operand](#operands) using the specified width as the target width.
If the target width is smaller than the source width the data between $width_{source}-width_{target}$ will be discarded.

```rust
{let} destination = Resize(operand,<target width>);
```

</details>

<details><summary> SetNFlag </summary>
Sets the `N` flag if the operand's most significant bit is one.

```rust
SetNFlag(operand);
```
</details>

<details><summary> SetZFlag </summary>
Sets the `Z` flag if the operand is equal to zero.

```rust
SetZFlag(operand);
```
</details>

<details><summary> LocalAddress </summary>

Creates a general assembly operand that represents an address whose value is stored in the local scope.
The function converts the operand name into a string and uses that to index the set of locals.

```rust
LocalAddress(operand,<width>) = <operation>; // Stores the result in memory
{let} destination = LocalAddress(operand,<width>); // Loads a value from memory
{let} destination = LocalAddress(operand,<width>) <operation> <operand2> ; // Uses the value stored in memory as part of a binary operation
{let} destination = <operand2> <operation> LocalAddress(operand,<width>) ; // Uses the value stored in memory as part of a binary operation

// Equivalent syntax with explicit naming

LocalAddress("operand",<width>) = <operation>; // Stores the result in memory
{let} destination = LocalAddress("operand",<width>); // Loads a value from memory
{let} destination = LocalAddress("operand",<width>) <operation> <operand> ; // Uses the value stored in memory as part of a binary operation
{let} destination = <operand> <operation> LocalAddress("operand",<width>) ; // Uses the value stored in memory as part of a binary operation
```

Note that this is not an exhaustive list of all cases where the function can be used, as it is a valid [operand](#operands) it has many more use cases.
</details>

<details><summary> SetVFlag </summary>
Sets the `V` flag if the operation results in overflow.

```rust
SetVFlag(operand1,operand2, <sub>, <carry>);
SetVFlag(operand1,operand2, add);
SetVFlag(operand1,operand2, adc);
SetVFlag(operand1,operand2, sub);
SetVFlag(operand1,operand2, sbc);
```
</details>

<details><summary> SetCFlag </summary>
Sets the `C` flag if the operation results in carry out.

```rust
SetCFlag(operand1,operand2, <sub>, <carry>);
SetCFlag(operand1,operand2, add);
SetCFlag(operand1,operand2, adc);
SetCFlag(operand1,operand2, sub);
SetCFlag(operand1,operand2, sbc);
SetCFlag(operand1,shift, lsl);
SetCFlag(operand1,shift, rsl);
SetCFlag(operand1,shift, rsa);
SetCFlag(operand1, ror);
```

</details>

<details><summary> Flag </summary>

Creates a general assembly operand that represents a CPU flag.
The function converts the operand name into a string and uses that to index the set of CPU flags.

```rust
LocalAddress("flag") = <operation>; // Set the value of a flag.
{let} destination = LocalAddress("flag"); // Copy the value of a flag
{let} destination = LocalAddress("flag") <operation> <operand> ; // Uses the a flag in an operation
```

Note that this is not an exhaustive list of all cases where the function can be used, as it is a valid [operand](#operands) it has many more use cases.
</details>

<details><summary> Register </summary>

Creates a general assembly operand that represents a CPU register.
The function converts the operand name into a string and uses that to index the set of CPU flags.

```rust
Register("operand") = <operation>; // Set the value of a register.
{let} destination = Register("operand",<width>); // Copy the value of a register.
{let} destination = Register("operand",<width>) <operation> <operand> ; // Uses the a register in an operation.
```

Note that this is not an exhaustive list of all cases where the function can be used, as it is a valid [operand](#operands) it has many more use cases.
</details>

<details><summary> Ror </summary>

Rotates the operand `N` steps to the right.

```rust
Ror(operand,n);
```

Note that this is not an exhaustive list of all cases where the function can be used, as it is a valid [operand](#operands) it has many more use cases.
</details>

<details><summary> Sra </summary>

Shifts the operand `N` steps to the right sign keeping the sign and filling in the gap with the sign bits value.

```rust
Sra(operand,n);
```

Note that this is not an exhaustive list of all cases where the function can be used, as it is a valid [operand](#operands) it has many more use cases.
</details>

<details><summary> Signed </summary>

Converts the contained [binary operation](#binary-operations) to its signed equivalent.

```rust
Signed(<operand> <operation> <operand>);
```

Note that this is not an exhaustive list of all cases where the function can be used, as it is a valid [operand](#operands) it has many more use cases.
</details>
</details>

## Building blocks

### Operands

There are a few operands defined in the language, these are as follows:

<details><summary> Identifier </summary>

A simple rust identifier that can optionally be declared inline in the pseudo-code.

```rust
{let} <identifier>
```

if the identifier is declared in-line it will be inserted right after the end of the most recent control flow instructions, if no such control flow instructions are present it will be inserted above the first invocation of the transpiler.
</details>

<details><summary> Expression operands </summary>

These operands are valid to use inside of expressions such as [binary operations](#binary-operations).

#### Parentheses

```rust
(<rust expression>)
```

#### Chains

These are compositions of Expression operands delimited by `.`.

```rust
<expression operand>.<expression operand>
```

#### Plain rust identifiers

```rust
<identifier>
```

#### Plain rust literals

```rust
<literal>
```

#### Function calls as operands

Inline function calls, see [function-calls](#function-calls).

#### Field extract

This is a meta instruction that masks out and right justifies the specified range of bits from the operand.

```rust
operand<<start>:<end>{:mask intermediate size}>
```

where the mask intermediate size defaults to u32.

</details>

## Contributing

If you find this project interesting and or useful feel free to contribute by either finding an open issue in the [issue tracker](https://github.com/ivario123/transpiler/issues) or opening a [`PR`](https://github.com/ivario123/transpiler/pulls) with fixes or features that you find useful.
Before contributing you should read the short [documentation](../CONTRIBUTING.md) on contributions.

## License

This repository is licensed under the [MIT](../LICENSE) license and any contributions shall be licensed under the same license unless explicitly stated otherwise.
