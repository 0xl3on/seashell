use seashell::{try_find_workspace_root, Seashell};
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;

#[test]
fn test_account_loader() {
    let mut seashell = Seashell::new();
    let account_loader_out_dir = try_find_workspace_root()
        .unwrap()
        .join("programs/account-loader/target/deploy");
    unsafe { std::env::set_var("SBF_OUT_DIR", account_loader_out_dir.to_str().unwrap()) }
    let program_id = Pubkey::new_unique();
    seashell
        .load_program_from_environment("account_loader", program_id)
        .unwrap();

    seashell.enable_log_collector();

    let mut pubkey_order = Vec::new();
    let account_metas: [AccountMeta; 50] = std::array::from_fn(|_| {
        let pubkey = Pubkey::new_unique();
        pubkey_order.push(pubkey);
        AccountMeta::new(pubkey, false)
    });

    for meta in &account_metas {
        seashell.set_account(
            meta.pubkey,
            Account {
                lamports: 1000,
                data: vec![],
                owner: Pubkey::new_unique(),
                executable: false,
                rent_epoch: 0,
            },
        );
    }

    let instruction = Instruction { program_id, accounts: account_metas.to_vec(), data: vec![] };

    seashell.process_instruction(instruction);

    let logs = seashell.logs().expect("log collector was set");

    let pubkeys: Vec<&str> = logs
        .iter()
        .skip(1)
        .filter_map(|line| line.split("Program log: ").last())
        .collect();

    for (pubkey_str, pubkey) in pubkeys.iter().zip(pubkey_order.iter()) {
        assert_eq!(pubkey_str, &pubkey.to_string())
    }
}

#[test]
fn test_account_loader_duplicate_accounts() {
    let mut seashell = Seashell::new();
    let account_loader_out_dir = try_find_workspace_root()
        .unwrap()
        .join("programs/account-loader/target/deploy");
    unsafe { std::env::set_var("SBF_OUT_DIR", account_loader_out_dir.to_str().unwrap()) }
    let program_id = Pubkey::new_unique();
    seashell
        .load_program_from_environment("account_loader", program_id)
        .unwrap();

    seashell.enable_log_collector();

    let mut pubkey_order = Vec::new();
    let duplicate = Pubkey::new_unique();
    let duplicate_2 = Pubkey::new_unique();
    println!("Duplicate 1: {duplicate}");
    println!("Duplicate 2: {duplicate_2}");
    seashell.set_account(duplicate, Account::default());
    seashell.set_account(duplicate_2, Account::default());
    let account_metas: [AccountMeta; 10] = std::array::from_fn(|pos| {
        if pos.is_multiple_of(5) {
            println!("{pos}: adding dup {duplicate}");
            pubkey_order.push(duplicate);
            AccountMeta::new_readonly(duplicate, false)
        } else if pos.is_multiple_of(2) {
            println!("{pos}: adding dup2 {duplicate_2}");
            pubkey_order.push(duplicate_2);
            AccountMeta::new_readonly(duplicate_2, false)
        } else {
            let pubkey = Pubkey::new_unique();
            println!("{pos}: adding random {pubkey}");
            seashell.set_account(pubkey, Account::default());
            pubkey_order.push(pubkey);
            AccountMeta::new(pubkey, false)
        }
    });

    let instruction = Instruction { program_id, accounts: account_metas.to_vec(), data: vec![] };

    seashell.process_instruction(instruction);

    let logs = seashell.logs().expect("log collector was set");

    let pubkeys: Vec<&str> = logs
        .iter()
        .skip(1)
        .filter_map(|line| line.split("Program log: ").last())
        .collect();

    for (pubkey_str, pubkey) in pubkeys.iter().zip(pubkey_order.iter()) {
        assert_eq!(pubkey_str, &pubkey.to_string())
    }
}

/// `clear_logs()` drops accumulated logs between `process_instruction` calls.
///
/// Without `clear_logs()`, successive calls share a single log collector
/// and `logs()` returns the union. Parsers that take the first matching
/// line can silently read stale content from an earlier call.
///
/// This test demonstrates both:
///   1. Without clear_logs: logs from ix_a and ix_b are both present
///      after the second call.
///   2. With clear_logs: only ix_b's logs remain after the second call.
#[test]
fn clear_logs_resets_between_process_instruction_calls() {
    let mut seashell = Seashell::new();
    let account_loader_out_dir = try_find_workspace_root()
        .unwrap()
        .join("programs/account-loader/target/deploy");
    unsafe { std::env::set_var("SBF_OUT_DIR", account_loader_out_dir.to_str().unwrap()) }
    let program_id = Pubkey::new_unique();
    seashell
        .load_program_from_environment("account_loader", program_id)
        .unwrap();
    seashell.enable_log_collector();

    // Two distinct pubkeys — each will be logged as its own line by the
    // account-loader program.
    let pk_a = Pubkey::new_unique();
    let pk_b = Pubkey::new_unique();
    seashell.set_account(pk_a, Account::default());
    seashell.set_account(pk_b, Account::default());

    let ix_a = Instruction {
        program_id,
        accounts: vec![AccountMeta::new_readonly(pk_a, false)],
        data: vec![],
    };
    let ix_b = Instruction {
        program_id,
        accounts: vec![AccountMeta::new_readonly(pk_b, false)],
        data: vec![],
    };

    // --- Without clear_logs: logs accumulate (documenting the footgun) ---
    seashell.process_instruction(ix_a.clone());
    seashell.process_instruction(ix_b.clone());
    let combined = seashell.logs().expect("log collector set");
    let combined_joined = combined.join("\n");
    assert!(combined_joined.contains(&pk_a.to_string()), "pk_a should be in combined logs");
    assert!(combined_joined.contains(&pk_b.to_string()), "pk_b should be in combined logs");

    // --- With clear_logs: logs reset between calls ---
    seashell.enable_log_collector(); // full reset for clean slate
    seashell.process_instruction(ix_a);
    let logs_a = seashell.logs().expect("log collector set");
    let logs_a_joined = logs_a.join("\n");
    assert!(logs_a_joined.contains(&pk_a.to_string()));
    assert!(!logs_a_joined.contains(&pk_b.to_string()));

    seashell.clear_logs();
    seashell.process_instruction(ix_b);
    let logs_b = seashell.logs().expect("log collector set");
    let logs_b_joined = logs_b.join("\n");
    assert!(
        logs_b_joined.contains(&pk_b.to_string()),
        "pk_b should be in logs_b (its instruction's pubkey)"
    );
    assert!(
        !logs_b_joined.contains(&pk_a.to_string()),
        "pk_a should NOT be in logs_b after clear_logs — that's the whole point. logs_b = {:?}",
        logs_b
    );
}
