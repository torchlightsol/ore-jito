use std::{
    io::{stdout, Write},
    time::Duration,
};
use solana_client::{
    client_error::{ClientError, ClientErrorKind, Result as ClientResult},
    nonblocking::rpc_client::RpcClient,
};
use solana_program::instruction::Instruction;
use solana_program::system_instruction::transfer;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    signature::{Signature, Signer},
    transaction::Transaction,
};
use solana_sdk::transaction::VersionedTransaction;
use solana_transaction_status::TransactionConfirmationStatus;
use jito_searcher_client::send_bundle_no_wait;
use crate::Miner;

const RPC_RETRIES: usize = 0;
const SIMULATION_RETRIES: usize = 4;
const GATEWAY_RETRIES: usize = 4;
const CONFIRM_RETRIES: usize = 4;

impl Miner {
    pub async fn send_and_confirm(
        &self,
        ixs: &[Instruction],
        skip_confirm: bool,
    ) -> ClientResult<Signature> {
        let mut stdout = stdout();
        let signer = self.signer();
        let jito_keypair = self.jito_keypair();
        let tip_account = self.tip_account();
        let mut jito_client = self.jito_client();
        let client =
            RpcClient::new_with_commitment(self.cluster.clone(), CommitmentConfig::confirmed());

        // Return error if balance is zero
        let balance = client
            .get_balance_with_commitment(&signer.pubkey(), CommitmentConfig::confirmed())
            .await
            .unwrap();

        if balance.value <= 0 {
            return Err(ClientError {
                request: None,
                kind: ClientErrorKind::Custom("Insufficient SOL balance".into()),
            });
        }

        // Build tx
        let (mut hash, mut slot) = client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .await
            .unwrap();

        let mut tx = Transaction::new_with_payer(
            &ixs,
            Some(&signer.pubkey()));
        // Submit tx
        tx.sign(&[&signer], hash);

        let mut tip = Transaction::new_with_payer(
            &[
                transfer(&jito_keypair.pubkey(), &tip_account, 1000000)
            ],
            Some(&jito_keypair.pubkey()));

        tip.sign(&[&jito_keypair], hash);

        let sig = tx.signatures[0];
        let txs = vec![VersionedTransaction::from(tx), VersionedTransaction::from(tip)];

        let mut sigs = vec![];
        let mut attempts = 0;
        loop {
            println!("Attempt: {:?}", attempts);
            match send_bundle_no_wait(
                &*txs,
                &mut jito_client,
            ).await {
                Ok(..) => {
                    println!("{:?}", sig);
                    sigs.push(sig);

                    // Confirm tx
                    if skip_confirm {
                        return Ok(sig);
                    }

                    for _ in 0..CONFIRM_RETRIES {
                        std::thread::sleep(Duration::from_millis(2000));
                        match client.get_signature_statuses(&sigs).await {
                            Ok(signature_statuses) => {
                                println!("Confirms: {:?}", signature_statuses.value);
                                for signature_status in signature_statuses.value {
                                    if let Some(signature_status) = signature_status.as_ref() {
                                        if signature_status.confirmation_status.is_some() {
                                            let current_commitment = signature_status
                                                .confirmation_status
                                                .as_ref()
                                                .unwrap();
                                            match current_commitment {
                                                TransactionConfirmationStatus::Processed => {}
                                                TransactionConfirmationStatus::Confirmed
                                                | TransactionConfirmationStatus::Finalized => {
                                                    println!("Transaction landed!");
                                                    return Ok(sig);
                                                }
                                            }
                                        } else {
                                            println!("No status");
                                        }
                                    }
                                }
                            }

                            // Handle confirmation errors
                            Err(err) => {
                                println!("Error: {:?}", err);
                            }
                        }
                    }
                    println!("Transaction did not land");
                }
                Err(err) => {
                    println!("Error {:?}", err);
                }
            };
            stdout.flush().ok();

            attempts += 1;
            if attempts > GATEWAY_RETRIES {
                return Err(ClientError {
                    request: None,
                    kind: ClientErrorKind::Custom("Max retries".into()),
                });
            }
        }
    }
}
