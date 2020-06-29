//
// @author Aurélien Nicolas <aurel@qed-it.com>
// @date 2019

use std::slice;
use std::error::Error;
use zkinterface::{
    reading::Messages,
    owned::circuit::CircuitOwned,
    owned::command::CommandOwned,
};

#[link(name = "zkif_gadgetlib", kind = "static")]
#[allow(improper_ctypes)]
extern "C" {
    fn gadgetlib_call_gadget(
        call_msg: *const u8,
        constraints_callback: extern fn(context_ptr: *mut Messages, message: *const u8) -> bool,
        constraints_context: *mut Messages,
        witness_callback: extern fn(context_ptr: *mut Messages, message: *const u8) -> bool,
        witness_context: *mut Messages,
        return_callback: extern fn(context_ptr: *mut Messages, message: *const u8) -> bool,
        return_context: *mut Messages,
    ) -> bool;
}

/// Collect the stream of any messages into the context.
extern "C"
fn receive_message_callback(
    context_ptr: *mut Messages,
    message_ptr: *const u8,
) -> bool {
    let (context, buf) = from_c(context_ptr, message_ptr);

    context.push_message(Vec::from(buf)).is_ok()
}


// Read a size prefix (4 bytes, little-endian).
fn read_size_prefix(ptr: *const u8) -> u32 {
    let buf = unsafe { slice::from_raw_parts(ptr, 4) };
    ((buf[0] as u32) << 0) | ((buf[1] as u32) << 8) | ((buf[2] as u32) << 16) | ((buf[3] as u32) << 24)
}

// Bring arguments from C calls back into the type system.
fn from_c<'a, CTX>(
    context_ptr: *mut CTX,
    response: *const u8,
) -> (&'a mut CTX, &'a [u8]) {
    let context = unsafe { &mut *context_ptr };

    let response_len = read_size_prefix(response) + 4;
    let buf = unsafe { slice::from_raw_parts(response, response_len as usize) };

    (context, buf)
}

pub fn call_gadget_wrapper(circuit: &CircuitOwned, command: &CommandOwned) -> Result<Messages, Box<dyn Error>> {
    let mut message_buf = vec![];
    circuit.write(&mut message_buf)?;
    command.write(&mut message_buf)?;

    let mut output_context = Messages::new(circuit.free_variable_id);
    let ok = unsafe {
        gadgetlib_call_gadget(
            message_buf.as_ptr(),
            receive_message_callback,
            &mut output_context as *mut Messages,
            receive_message_callback,
            &mut output_context as *mut Messages,
            receive_message_callback,
            &mut output_context as *mut Messages,
        )
    };

    match ok {
        true => Ok(output_context),
        false => Err("call_gadget failed".into()),
    }
}


#[test]
fn test_cpp_gadget() {
    use zkinterface::owned::variables::VariablesOwned;

    let mut subcircuit = CircuitOwned {
        connections: VariablesOwned {
            variable_ids: vec![100, 101, 102, 103], // Some input variables.
            values: None,
        },
        free_variable_id: 104,
        field_maximum: None,
    };


    println!("==== R1CS generation ====");
    let command = CommandOwned { constraints_generation: true, witness_generation: false };
    let r1cs_response = call_gadget_wrapper(&subcircuit, &command).unwrap();

    println!("R1CS: Rust received {} messages including {} gadget return.",
             r1cs_response.messages.len(),
             r1cs_response.circuits().len());

    assert!(r1cs_response.messages.len() == 2);
    assert!(r1cs_response.circuits().len() == 1);

    println!("R1CS: Got constraints:");
    for c in r1cs_response.iter_constraints() {
        println!("{:?} * {:?} = {:?}", c.a, c.b, c.c);
    }

    let free_variable_id_after = r1cs_response.last_circuit().unwrap().free_variable_id();
    println!("R1CS: Free variable id after the call: {}\n", free_variable_id_after);
    assert!(free_variable_id_after == 104 + 40);


    println!("==== Witness generation ====");
    // Specify input values.
    subcircuit.connections.values = Some(vec![4, 5, 6, 14, 15, 16 as u8]);

    let command = CommandOwned { constraints_generation: false, witness_generation: true };
    let witness_response = call_gadget_wrapper(&subcircuit, &command).unwrap();

    println!("Assignment: Rust received {} messages including {} gadget return.",
             witness_response.messages.len(),
             witness_response.circuits().len());

    assert!(witness_response.messages.len() == 2);
    assert!(witness_response.circuits().len() == 1);

    {
        let assignment: Vec<_> = witness_response.iter_witness().collect();

        println!("Assignment: Got witness:");
        for var in assignment.iter() {
            println!("{} = {:?}", var.id, var.value);
        }

        assert_eq!(assignment.len(), 40);
        assert_eq!(assignment[0].value.len(), 3);
        assert_eq!(assignment[0].id, 103 + 0); // First gadget-allocated variable.
        assert_eq!(assignment[1].id, 103 + 1); // Second "
        assert_eq!(assignment[0].value, &[10, 11, 12]); // First element.
        assert_eq!(assignment[1].value, &[8, 7, 6]);    // Second element

        let free_variable_id_after2 = witness_response.last_circuit().unwrap().free_variable_id();
        println!("Assignment: Free variable id after the call: {}", free_variable_id_after2);
        assert!(free_variable_id_after2 == 102 + 1 + 2);
        assert!(free_variable_id_after2 == free_variable_id_after);

        let out_vars = witness_response.connection_variables().unwrap();
        println!("{:?}", out_vars);
    }
    println!();
}
