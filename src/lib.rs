// Allow `cargo stylus export-abi` to generate a main function.
#![cfg_attr(not(feature = "export-abi"), no_main)]
extern crate alloc;

/// Use an efficient WASM allocator.
#[global_allocator]
static ALLOC: mini_alloc::MiniAlloc = mini_alloc::MiniAlloc::INIT;
/// Import items from the SDK. The prelude contains common traits and macros.
use stylus_sdk::{alloy_primitives::{Address, U256}, alloy_sol_types::sol_data, block, call::{transfer_eth, Call}, contract, evm, msg, prelude::*};
use alloy_sol_types::{sol, SolInterface, SolType};


sol_interface! {
    interface IERC721 {
        function safeTransferFrom(address from, address to, uint256 tokenId) external;
        function transferFrom(address, address, uint256) external;
    }
}

sol!{
    event Start();
    event Bid(address indexed sender, uint256 amount);
    event Withdraw(address indexed bidder, uint256 amount);
    event End(address winner, uint256 amount);

    error AlreadyStarted();
    error NotSeller();
    error AuctionEnded();
    error BidTooLow();
    error NotStarted();
    error NotEnded();
}

#[derive(SolidityError)]
pub enum EnglishAuctionError {
    AlreadyStarted(AlreadyStarted),
    NotSeller(NotSeller),
    AuctionEnded(AuctionEnded),
    BidTooLow(BidTooLow),
    NotStarted(NotStarted),
    NotEnded(NotEnded),
}

// Define some persistent storage using the Solidity ABI.
// `Counter` will be the entrypoint.
sol_storage! {
    #[entrypoint]
    pub struct EnglishAuction {
        address nftAddress;
        uint256 nftId;

        address seller;
        uint256 endAt;
        bool started;
        bool ended;

        address highestBidder;
        uint256 highestBid;
        mapping(address => uint256) bids;
    }
}

/// Declare that `Counter` is a contract with the following external methods.
#[external]
impl EnglishAuction {
    pub const ONE_DAY: u64 = 24 * 60 * 60;

    pub fn start(&mut self) -> Result<(), EnglishAuctionError> {
        if self.started.get() {
            return Err(EnglishAuctionError::AlreadyStarted(AlreadyStarted{}));
        }
        
        if self.seller.get() != msg::sender() {
            return Err(EnglishAuctionError::NotSeller(NotSeller{}));
        }
        
        let nft = IERC721::new(*self.nftAddress);
        let nft_id = self.nftId.get();

        
        let config = Call::new();
        let result = nft.transfer_from(config, msg::sender(), contract::address(), nft_id);
        
        match result {
            Ok(_) => {
                self.started.set(true);
                self.endAt.set(U256::from(block::timestamp() + 7 * Self::ONE_DAY));
                evm::log(Start {});
                Ok(())
            },
            Err(_) => {
                return Err(EnglishAuctionError::NotSeller(NotSeller{}));
            }
            
        }
    }

    #[payable]
    pub fn bid(&mut self) -> Result<(), EnglishAuctionError> {
        if !self.started.get() {
            return Err(EnglishAuctionError::NotSeller(NotSeller{}));
        }
        
        if U256::from(block::timestamp()) >= self.endAt.get() {
            return Err(EnglishAuctionError::AuctionEnded(AuctionEnded{}));
        }
        
        if msg::value() <= self.highestBid.get() {
            return Err(EnglishAuctionError::BidTooLow(BidTooLow{}));
        }
        
        if self.highestBidder.get() != Address::default() {
            let mut bid = self.bids.setter(self.highestBidder.get());
            let current_bid = bid.get();
            bid.set(current_bid + self.highestBid.get());
        }
        
        self.highestBidder.set(msg::sender());
        self.highestBid.set(msg::value());

        evm::log(Bid {
            sender: msg::sender(),
            amount: msg::value(),
        });
        Ok(())
    }

    pub fn withdraw(&mut self) -> Result<(), EnglishAuctionError> {
        let mut current_bid = self.bids.setter(msg::sender());
        let bal = current_bid.get();
        current_bid.set(U256::from(0));
        transfer_eth(msg::sender(), bal);

        evm::log(Withdraw {
            bidder: msg::sender(),
            amount: bal,
        });
        Ok(())
    }

    pub fn end(&mut self) -> Result<(), EnglishAuctionError> {
        if !self.started.get() {
            return Err(EnglishAuctionError::NotStarted(NotStarted{}));
        }
        
        if U256::from(block::timestamp()) < self.endAt.get() {
            return Err(EnglishAuctionError::NotEnded(NotEnded{}));
        }
        
        if self.ended.get() {
            return Err(EnglishAuctionError::AuctionEnded(AuctionEnded{}));
        }
        
        self.ended.set(true);

        let seller_address = self.seller.get();
        let highest_bid = self.highestBid.get();
        let highest_bidder = self.highestBidder.get();
        let nft_id = self.nftId.get();
        let config = Call::new();
        if self.highestBidder.get() != Address::default() {
            let nft = IERC721::new(*self.nftAddress);
            nft.safe_transfer_from(config, contract::address(), highest_bidder, nft_id);
            transfer_eth(seller_address, highest_bid);
        } else {
            let nft = IERC721::new(*self.nftAddress);
            nft.safe_transfer_from(config, contract::address(), seller_address, nft_id);
        }

        evm::log(End {
            winner: highest_bidder,
            amount: highest_bid,
        });
        Ok(())
    }
}
