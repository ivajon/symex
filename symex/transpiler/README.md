<h1 align="center">
  Symex General Assembly Transpiler
</h1>

This repository defines a small DSL used to define [symex](../../) general_assembly instructions more cleanly and shortly rather than writing out struct by hand. This crate only makes sense in the context of Symex as of now.

## Usage

<details><summary>:warning:  **All declarations are inserted before the start of the most recent block** </summary>

   This means that the code

   ```rust
   pseudo!([
        let a = b + c;
   ])
   ```

   inserts the declaration of `a` right above the call to `pseudo`
   while the code

   ```rust
   let s = false;
   pseudo!([
    let a = b + c;
    if (s) {
        let d = a + c;
    }
   ])
   ```

   inserts the declaration of `d` inside the generated `if` statement.
    Finally, code like

   ```rust
   let s = false;
   pseudo!([
    let a = b + c;
    if (s) {
        let d = a + c;
    }
    let f = d;
   ])
   ```

   inserts the declaration of `f` right after the `if` block ends.
   This allows scoping of variables but not shadowing of variables.
</details>

### Examples

<details>
  <summary> Move </summary>

Instead of writing

```rust
ret.push(Operation::Move {
    destination: rd.clone(),
    source: rn.clone()
});
```

We can now write

```rust
pseudo!(ret.extend[    
    rd = rn
]);
```

</details>

<details>
  <summary>Conditional add or subtract</summary>
Instead of writing

```rust
if add {
    ret.push(Operation::Add {
        destination: rd.clone(),
        operand1: rn.clone(),
        operand2: rm.clone()
    });
} else {
    ret.push(Operation::Sub {
        destination: rd.clone(),
        operand1: rn.clone(),
        operand2: rm.clone()
    });
}
```

We can now write

```rust
pseudo!(ret.extend[    
    if (add) {
        rd = rn + rm;
    }
    else {
        rd = rn - rm;
    }
]);
```

</details>

<details>
  <summary>Add 1 to vector of registers</summary>

Assuming that the registers implement `Into<Operand>`.
And that we implement some `LocalInto<Operand>` for u32.
Instead of writing

```rust
for register in registers {
    ret.push(
        Operation::Add {
            destination: rd.clone(),
            operand1: register.into(),
            operand2: 1.local_into()
        }
    )
}
```

We can now write

```rust
pseudo!(ret.extend[ 
    for register in registers {
        rd = register.into() + 1.local_into();
    }
]);
```

</details>

### A bit more involved examples

I am not going to write these out in struct form as that would take up a lot of space.

<details>
  <summary> Add together the msh and lsh of a register </summary>

```rust
let ret = pseudo!([
    let result = register<31:16> + register<15:0>;
]);
```

</details>

<details>
  <summary> Branch to XOR result between to register if Jump is true </summary>

```rust
let ret = pseudo!([
    let result = rn ^ rm;

    if(Jump) {
        Jump(result)
    }
]);
```

</details>

<details>
  <summary> Adc but set flags if S is true </summary>

```rust
let ret = pseudo!([
    let result = rn adc rm;

    if(s) {
        SetZFlag(result);
        SetNFlag(result);
        // Can be add, adc, sbc, sub
        SetCFlag(rn,rm,adc);
        SetVFlag(rn,rm,adc);
    }

    rd = result;
]);
```

</details>

for a more detailed look at the language features please read the [language documentation](./language/README.md)

## License

This repository is licensed under the same license as the [symex project](../../) and any contributions shall be licensed under the same license unless explicitly stated otherwise.

