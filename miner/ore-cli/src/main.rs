mod balance;
mod busses;
mod claim;
mod cu_limits;
#[cfg(feature = "admin")]
mod initialize;
mod mine;
mod register;
mod rewards;
mod send_and_confirm;
mod treasury;
#[cfg(feature = "admin")]
mod update_admin;
#[cfg(feature = "admin")]
mod update_difficulty;
mod utils;

use std::str::FromStr;
use std::sync::Arc;
use clap::{command, Parser, Subcommand};
use solana_program::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use tonic::codegen::InterceptedService;
use tonic::transport::Channel;
use jito_protos::searcher::searcher_service_client::SearcherServiceClient;
use jito_searcher_client::{
    get_searcher_client, token_authenticator::ClientInterceptor,
};

struct Miner {
    pub keypair: Keypair,
    pub jito_keypair: Keypair,
    pub priority_fee: u64,
    pub cluster: String,
    pub regions: Vec<String>,
    pub tip_account: Pubkey,
    pub jito_client: SearcherServiceClient<InterceptedService<Channel, ClientInterceptor>>,
}

#[derive(Parser, Debug)]
#[command(about, version)]
struct Args {
    #[arg(
        long,
        value_name = "NETWORK_URL",
        help = "Network address of your RPC provider",
    )]
    rpc: String,

    /// URL of the block engine.
    /// See: https://jito-labs.gitbook.io/mev/searcher-resources/block-engine#connection-details
    #[arg(long)]
    block_engine_url: String,

    /// Comma-separated list of regions to request cross-region data from.
    /// If no region specified, then default to the currently connected block engine's region.
    /// Details: https://jito-labs.gitbook.io/mev/searcher-services/recommendations#cross-region
    /// Available regions: https://jito-labs.gitbook.io/mev/searcher-resources/block-engine#connection-details
    #[arg(long, value_delimiter = ',')]
    regions: Vec<String>,

    #[arg(
        long,
        value_name = "PRIVATE_KEY",
        help = "Private key to use",
    )]
    private_key: String,

    #[arg(
        long,
        value_name = "JITO_PRIVATE_KEY",
        help = "Jito private key to use",
    )]
    jito_private_key: String,

    #[arg(
        long,
        value_name = "MICROLAMPORTS",
        help = "Number of microlamports to pay as priority fee per transaction",
        default_value = "0",
    )]
    priority_fee: u64,

    #[arg(
        long,
        value_name = "TIP_ACCOUNT",
        help = "Jito tip account public key",
    )]
    tip_account: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Fetch the Ore balance of an account")]
    Balance(BalanceArgs),

    #[command(about = "Fetch the distributable rewards of the busses")]
    Busses(BussesArgs),

    #[command(about = "Mine Ore using local compute")]
    Mine(MineArgs),

    #[command(about = "Claim available mining rewards")]
    Claim(ClaimArgs),

    #[command(about = "Fetch your balance of unclaimed mining rewards")]
    Rewards(RewardsArgs),

    #[command(about = "Fetch the treasury account and balance")]
    Treasury(TreasuryArgs),

    #[cfg(feature = "admin")]
    #[command(about = "Initialize the program")]
    Initialize(InitializeArgs),

    #[cfg(feature = "admin")]
    #[command(about = "Update the program admin authority")]
    UpdateAdmin(UpdateAdminArgs),

    #[cfg(feature = "admin")]
    #[command(about = "Update the mining difficulty")]
    UpdateDifficulty(UpdateDifficultyArgs),
}

#[derive(Parser, Debug)]
struct BalanceArgs {
    #[arg(
        // long,
        value_name = "ADDRESS",
        help = "The address of the account to fetch the balance of"
    )]
    pub address: Option<String>,
}

#[derive(Parser, Debug)]
struct BussesArgs {}

#[derive(Parser, Debug)]
struct RewardsArgs {
    #[arg(
        // long,
        value_name = "ADDRESS",
        help = "The address of the account to fetch the rewards balance of"
    )]
    pub address: Option<String>,
}

#[derive(Parser, Debug)]
struct MineArgs {
    #[arg(
        long,
        short,
        value_name = "THREAD_COUNT",
        help = "The number of threads to dedicate to mining",
        default_value = "1"
    )]
    threads: u64,
}

#[derive(Parser, Debug)]
struct TreasuryArgs {}

#[derive(Parser, Debug)]
struct ClaimArgs {
    #[arg(
        // long,
        value_name = "AMOUNT",
        help = "The amount of rewards to claim. Defaults to max."
    )]
    amount: Option<f64>,

    #[arg(
        // long,
        value_name = "TOKEN_ACCOUNT_ADDRESS",
        help = "Token account to receive mining rewards."
    )]
    beneficiary: Option<String>,
}

#[cfg(feature = "admin")]
#[derive(Parser, Debug)]
struct InitializeArgs {}

#[cfg(feature = "admin")]
#[derive(Parser, Debug)]
struct UpdateAdminArgs {
    new_admin: String,
}

#[cfg(feature = "admin")]
#[derive(Parser, Debug)]
struct UpdateDifficultyArgs {}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let jito_key_pair = Arc::new(Keypair::from_base58_string(&*args.jito_private_key));
    let jito_key_pair_1 = Keypair::from_base58_string(&*args.jito_private_key);
    let ore_key_pair = Keypair::from_base58_string(&*args.private_key);
    let tip_account = Pubkey::from_str(&*args.tip_account).unwrap();

    let client = get_searcher_client(&*args.block_engine_url, &jito_key_pair)
        .await
        .expect("connects to searcher client");

    // Initialize miner.
    let cluster = args.rpc;

    let miner = Arc::new(Miner::new(
        cluster.clone(),
        jito_key_pair_1,
        args.priority_fee,
        args.regions,
        ore_key_pair,
        client,
        tip_account,
    ));

    // Execute user command.
    match args.command {
        Commands::Balance(args) => {
            miner.balance(args.address).await;
        }
        Commands::Busses(_) => {
            miner.busses().await;
        }
        Commands::Rewards(args) => {
            miner.rewards(args.address).await;
        }
        Commands::Treasury(_) => {
            miner.treasury().await;
        }
        Commands::Mine(args) => {
            miner.mine(args.threads).await;
        }
        Commands::Claim(args) => {
            miner.claim(cluster, args.beneficiary, args.amount).await;
        }
        #[cfg(feature = "admin")]
        Commands::Initialize(_) => {
            miner.initialize().await;
        }
        #[cfg(feature = "admin")]
        Commands::UpdateAdmin(args) => {
            miner.update_admin(args.new_admin).await;
        }
        #[cfg(feature = "admin")]
        Commands::UpdateDifficulty(_) => {
            miner.update_difficulty().await;
        }
    }
}

impl Miner {
    pub fn new(cluster: String, jito_keypair: Keypair, priority_fee: u64, regions: Vec<String>, keypair: Keypair, jito_client: SearcherServiceClient<InterceptedService<Channel, ClientInterceptor>>, tip_account: Pubkey) -> Self {
        Self {
            keypair,
            jito_keypair,
            priority_fee,
            cluster,
            regions,
            jito_client,
            tip_account
        }
    }

    pub fn jito_keypair(&self) -> Keypair {
        return self.jito_keypair.insecure_clone();
    }

    pub fn signer(&self) -> Keypair {
        return self.keypair.insecure_clone();
    }

    pub fn regions(&self) -> Vec<String> {
        return self.regions.clone();
    }

    pub fn jito_client(&self) -> SearcherServiceClient<InterceptedService<Channel, ClientInterceptor>> {
        return self.jito_client.clone();
    }

    pub fn tip_account(&self) -> Pubkey {
        return self.tip_account.clone();
    }
}
