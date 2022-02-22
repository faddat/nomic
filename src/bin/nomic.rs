#![feature(trivial_bounds)]
#![allow(incomplete_features)]
#![feature(specialization)]
#![feature(async_closure)]
#![feature(never_type)]

use bitcoincore_rpc::{Auth, Client as BtcClient};
use clap::Parser;
use nomic::app::*;
use nomic::bitcoin::relayer::Relayer;
use orga::prelude::*;
use serde::{Deserialize, Serialize};

pub fn app_client() -> TendermintClient<nomic::app::App> {
    TendermintClient::new("http://localhost:26657").unwrap()
}

fn my_address() -> Address {
    let privkey = load_privkey().unwrap();
    let pubkey = secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &privkey);
    Address::from_pubkey(pubkey.serialize())
}

#[derive(Parser, Debug)]
#[clap(
    version = env!("CARGO_PKG_VERSION"),
    author = "The Nomic Developers <hello@nomic.io>"
)]
pub struct Opts {
    #[clap(subcommand)]
    cmd: Command,
}

#[derive(Parser, Debug)]
pub enum Command {
    Init(InitCmd),
    Start(StartCmd),
    Send(SendCmd),
    Balance(BalanceCmd),
    Delegations(DelegationsCmd),
    Validators(ValidatorsCmd),
    Delegate(DelegateCmd),
    Declare(DeclareCmd),
    Unbond(UnbondCmd),
    Claim(ClaimCmd),
    ClaimAirdrop(ClaimAirdropCmd),
    Relayer(RelayerCmd),
}

impl Command {
    async fn run(&self) -> Result<()> {
        use Command::*;
        match self {
            Init(cmd) => cmd.run().await,
            Start(cmd) => cmd.run().await,
            Send(cmd) => cmd.run().await,
            Balance(cmd) => cmd.run().await,
            Delegate(cmd) => cmd.run().await,
            Declare(cmd) => cmd.run().await,
            Delegations(cmd) => cmd.run().await,
            Validators(cmd) => cmd.run().await,
            Unbond(cmd) => cmd.run().await,
            Claim(cmd) => cmd.run().await,
            ClaimAirdrop(cmd) => cmd.run().await,
            Relayer(cmd) => cmd.run().await,
        }
    }
}

#[derive(Parser, Debug)]
pub struct InitCmd {}

impl InitCmd {
    async fn run(&self) -> Result<()> {
        tokio::task::spawn_blocking(|| {
            Node::<nomic::app::App>::new(CHAIN_ID);
        })
        .await
        .map_err(|err| orga::Error::App(err.to_string()))?;
        Ok(())
    }
}

#[derive(Parser, Debug)]
pub struct StartCmd {}

impl StartCmd {
    async fn run(&self) -> Result<()> {
        tokio::task::spawn_blocking(|| {
            Node::<nomic::app::App>::new(CHAIN_ID)
                // .with_genesis(include_bytes!("../genesis.json"))
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .reset()
                .run()
        })
        .await
        .map_err(|err| orga::Error::App(err.to_string()))?;
        Ok(())
    }
}

#[derive(Parser, Debug)]
pub struct SendCmd {
    to_addr: Address,
    amount: u64,
}

impl SendCmd {
    async fn run(&self) -> Result<()> {
        app_client()
            .pay_from(async move |mut client| client.accounts.take_as_funding(MIN_FEE.into()).await)
            .accounts
            .transfer(self.to_addr, self.amount.into())
            .await
    }
}

#[derive(Parser, Debug)]
pub struct BalanceCmd;

impl BalanceCmd {
    async fn run(&self) -> Result<()> {
        let address = my_address();
        println!("address: {}", address);

        let balance = app_client().accounts.balance(address).await??;
        println!("balance: {} NOM", balance);

        Ok(())
    }
}

#[derive(Parser, Debug)]
pub struct DelegationsCmd;

impl DelegationsCmd {
    async fn run(&self) -> Result<()> {
        let address = my_address();
        let delegations = app_client().staking.delegations(address).await??;

        println!(
            "delegated to {} validator{}",
            delegations.len(),
            if delegations.len() == 1 { "" } else { "s" }
        );
        for (validator, delegation) in delegations {
            let staked: u64 = delegation.staked.into();
            let liquid: u64 = delegation.liquid.into();
            if staked + liquid == 0 {
                continue;
            }
            println!(
                "- {}: staked={} NOM, liquid={} NOM",
                validator, staked, liquid
            );
        }

        Ok(())
    }
}

#[derive(Parser, Debug)]
pub struct ValidatorsCmd;

impl ValidatorsCmd {
    async fn run(&self) -> Result<()> {
        let validators = app_client().staking.all_validators().await??;

        for validator in validators {
            let info: DeclareInfo =
                serde_json::from_slice(validator.info.bytes.as_slice()).unwrap();
            println!(
                "- {}\n\tVOTING POWER: {}\n\tMONIKER: {}\n\tDETAILS: {}",
                validator.address, validator.amount_staked, info.moniker, info.details
            );
        }

        Ok(())
    }
}

#[derive(Parser, Debug)]
pub struct DelegateCmd {
    validator_addr: Address,
    amount: u64,
}

impl DelegateCmd {
    async fn run(&self) -> Result<()> {
        app_client()
            .pay_from(async move |mut client| {
                client
                    .accounts
                    .take_as_funding((self.amount + MIN_FEE).into())
                    .await
            })
            .staking
            .delegate_from_self(self.validator_addr, self.amount.into())
            .await
    }
}

#[derive(Parser, Debug)]
pub struct DeclareCmd {
    consensus_key: String,
    amount: u64,
    commission_rate: Decimal,
    moniker: String,
    website: String,
    identity: String,
    details: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeclareInfo {
    moniker: String,
    website: String,
    identity: String,
    details: String,
}

impl DeclareCmd {
    async fn run(&self) -> Result<()> {
        use std::convert::TryInto;
        let consensus_key: [u8; 32] = base64::decode(&self.consensus_key)
            .map_err(|_| orga::Error::App("invalid consensus key".to_string()))?
            .try_into()
            .map_err(|_| orga::Error::App("invalid consensus key".to_string()))?;

        let info = DeclareInfo {
            moniker: self.moniker.clone(),
            website: self.website.clone(),
            identity: self.identity.clone(),
            details: self.details.clone(),
        };
        let info_json = serde_json::to_string(&info)
            .map_err(|_| orga::Error::App("invalid json".to_string()))?;
        let info_bytes = info_json.as_bytes().to_vec();

        app_client()
            .pay_from(async move |mut client| {
                client
                    .accounts
                    .take_as_funding((self.amount + MIN_FEE).into())
                    .await
            })
            .staking
            .declare_self(
                consensus_key,
                self.commission_rate,
                self.amount.into(),
                info_bytes.into(),
            )
            .await
    }
}

#[derive(Parser, Debug)]
pub struct UnbondCmd {
    validator_addr: Address,
    amount: u64,
}

impl UnbondCmd {
    async fn run(&self) -> Result<()> {
        app_client()
            .pay_from(async move |mut client| client.accounts.take_as_funding(MIN_FEE.into()).await)
            .staking
            .unbond_self(self.validator_addr, self.amount.into())
            .await
    }
}

#[derive(Parser, Debug)]
pub struct ClaimCmd;

impl ClaimCmd {
    async fn run(&self) -> Result<()> {
        app_client()
            .pay_from(async move |mut client| client.staking.claim_all().await)
            .accounts
            .give_from_funding_all()
            .await
    }
}

#[derive(Parser, Debug)]
pub struct ClaimAirdropCmd;

impl ClaimAirdropCmd {
    async fn run(&self) -> Result<()> {
        app_client()
            .pay_from(async move |mut client| client.atom_airdrop.claim().await)
            .accounts
            .give_from_funding_all()
            .await
    }
}

#[derive(Parser, Debug)]
pub struct RelayerCmd {
    #[clap(short = 'p', long, default_value_t = 8332)]
    rpc_port: u16,

    #[clap(short = 'u', long)]
    rpc_user: Option<String>,

    #[clap(short = 'P', long)]
    rpc_pass: Option<String>,
}

impl RelayerCmd {
    fn btc_client(&self) -> Result<BtcClient> {
        let rpc_url = format!("http://localhost:{}", self.rpc_port);
        let auth = match (self.rpc_user.clone(), self.rpc_pass.clone()) {
            (Some(user), Some(pass)) => Auth::UserPass(user, pass),
            _ => Auth::None,
        };

        let btc_client = BtcClient::new(&rpc_url, auth)
            .map_err(|e| orga::Error::App(e.to_string()))?;

        Ok(btc_client)
    }

    async fn run(&self) -> Result<()> {
        let btc_client = self.btc_client()?;
        let app_client = app_client().bitcoin.clone();

        let mut relayer = Relayer::new(btc_client, app_client);
        relayer
            .start()
            .await
            .map_err(|e| orga::Error::App(e.to_string()))?;
    }
}

#[tokio::main]
async fn main() {
    let opts = Opts::parse();
    if let Err(err) = opts.cmd.run().await {
        eprintln!("{}", err);
        std::process::exit(1);
    };
}