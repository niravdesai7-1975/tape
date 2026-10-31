//! A test USDC (NEP-141, 6 decimals) for the local sandbox and testnet demo. Anyone can mint and register — test only.
use near_contract_standards::fungible_token::metadata::{FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC};
use near_contract_standards::fungible_token::{FungibleToken, FungibleTokenCore, FungibleTokenResolver};
use near_sdk::json_types::U128;
use near_sdk::{near, AccountId, PanicOnDefault, PromiseOrValue};

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Usdc { token: FungibleToken }

#[near]
impl Usdc {
    #[init]
    pub fn new() -> Self { Self { token: FungibleToken::new(b"t".to_vec()) } }
    /// Test only: register an account without a storage deposit.
    pub fn register(&mut self, account_id: AccountId) {
        if !self.token.accounts.contains_key(&account_id) { self.token.internal_register_account(&account_id); }
    }
    /// Test only: mint to any account.
    pub fn mint(&mut self, account_id: AccountId, amount: U128) {
        self.register(account_id.clone());
        self.token.internal_deposit(&account_id, amount.0);
    }
}

#[near]
impl FungibleTokenCore for Usdc {
    #[payable]
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) { self.token.ft_transfer(receiver_id, amount, memo) }
    #[payable]
    fn ft_transfer_call(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>, msg: String) -> PromiseOrValue<U128> {
        self.token.ft_transfer_call(receiver_id, amount, memo, msg)
    }
    fn ft_total_supply(&self) -> U128 { self.token.ft_total_supply() }
    fn ft_balance_of(&self, account_id: AccountId) -> U128 { self.token.ft_balance_of(account_id) }
}

#[near]
impl FungibleTokenResolver for Usdc {
    #[private]
    fn ft_resolve_transfer(&mut self, sender_id: AccountId, receiver_id: AccountId, amount: U128) -> U128 {
        let (used, _burned) = self.token.internal_ft_resolve_transfer(&sender_id, receiver_id, amount);
        used.into()
    }
}

#[near]
impl FungibleTokenMetadataProvider for Usdc {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        FungibleTokenMetadata { spec: FT_METADATA_SPEC.to_string(), name: "Test USDC".into(), symbol: "USDC".into(),
                                icon: None, reference: None, reference_hash: None, decimals: 6 }
    }
}
