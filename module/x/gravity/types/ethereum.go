package types

import (
	"bytes"
	"fmt"
	"regexp"
	"strings"

	sdk "github.com/cosmos/cosmos-sdk/types"
	sdkerrors "github.com/cosmos/cosmos-sdk/types/errors"
)

const (
	// GravityDenomPrefix indicates the prefix for all assests minted by this module
	GravityDenomPrefix = ModuleName

	// GravityDenomSeparator is the separator for gravity denoms
	GravityDenomSeparator = ""

	// ETHContractAddressLen is the length of contract address strings
	ETHContractAddressLen = 42

	// GravityDenomLen is the length of the denoms generated by the gravity module
	GravityDenomLen = len(GravityDenomPrefix) + len(GravityDenomSeparator) + ETHContractAddressLen

	// ZeroAddress is an EthAddress containing 0x0000000000000000000000000000000000000000
	ZeroAddressString = "0x0000000000000000000000000000000000000000"
)

// Regular EthAddress

func NewEthAddress(address string) (*EthAddress, error) {
	addr := EthAddress{address}
	if err := addr.ValidateBasic(); err != nil {
		return nil, sdkerrors.Wrap(err, "invalid input address")
	}
	return &addr, nil
}

func ZeroAddress() *EthAddress {
	return &EthAddress{ZeroAddressString}
}

func (ea EthAddress) ValidateBasic() error {
	if ea.Address == "" {
		return fmt.Errorf("empty")
	}
	if !regexp.MustCompile("^0x[0-9a-fA-F]{40}$").MatchString(ea.Address) {
		return fmt.Errorf("address(%s) doesn't pass regex", ea.Address)
	}
	if len(ea.Address) != ETHContractAddressLen {
		return fmt.Errorf("address(%s) of the wrong length exp(%d) actual(%d)", ea.Address, len(ea.Address), ETHContractAddressLen)
	}
	return nil
}

// EthAddrLessThan migrates the Ethereum address less than function
func EthAddrLessThan(e *EthAddress, o *EthAddress) bool {
	return bytes.Compare([]byte(e.Address)[:], []byte(o.Address)[:]) == -1
}

// Nillable EthAddress

func (o *OptionalEthAddress) Unwrap() (*EthAddress, error) {
	if o.IsNil {
		return nil, fmt.Errorf("nil value")
	}
	return o.Optional, nil
}

func (o *OptionalEthAddress) SetEthAddress(ethAddress *EthAddress) {
	if ethAddress == nil {
		o.IsNil = true
		o.Optional = nil
		return
	}

	o.IsNil = false
	o.Optional = ethAddress
}

func NilEthAddress() *OptionalEthAddress {
	return &OptionalEthAddress{
		IsNil:    true,
		Optional: nil,
	}
}

// Creates a new OptionalEthAddress, returns any error from calling NewEthAddress
func NewOptionalEthAddress(address string) (*OptionalEthAddress, error) {
	ethAddress, err := NewEthAddress(address)
	return &OptionalEthAddress{
		IsNil:    ethAddress == nil,
		Optional: ethAddress,
	}, err
}

/////////////////////////
//     ERC20Token      //
/////////////////////////

// NewERC20Token returns a new instance of an ERC20
func NewERC20Token(amount uint64, contract EthAddress) *ERC20Token {
	return &ERC20Token{Amount: sdk.NewIntFromUint64(amount), Contract: &contract}
}

func NewSDKIntERC20Token(amount sdk.Int, contract EthAddress) *ERC20Token {
	return &ERC20Token{Amount: amount, Contract: &contract}
}

// GravityCoin returns the gravity representation of the ERC20
func (e *ERC20Token) GravityCoin() sdk.Coin {
	return sdk.NewCoin(GravityDenom(e.Contract), e.Amount)
}

func GravityDenom(tokenContract *EthAddress) string {
	return fmt.Sprintf("%s%s%s", GravityDenomPrefix, GravityDenomSeparator, tokenContract.Address)
}

// ValidateBasic permforms stateless validation
func (e *ERC20Token) ValidateBasic() error {
	if err := e.Contract.ValidateBasic(); err != nil {
		return sdkerrors.Wrap(err, "ethereum address")
	}
	// TODO: Validate all the things
	return nil
}

// Add adds one ERC20 to another
// TODO: make this return errors instead
func (e *ERC20Token) Add(o *ERC20Token) *ERC20Token {
	if string(e.Contract.Address) != string(o.Contract.Address) {
		panic("invalid contract address")
	}
	sum := e.Amount.Add(o.Amount)
	if !sum.IsUint64() {
		panic("invalid amount")
	}
	return NewERC20Token(sum.Uint64(), *e.Contract)
}

func GravityDenomToERC20(denom string) (*EthAddress, error) {
	fullPrefix := GravityDenomPrefix + GravityDenomSeparator
	if !strings.HasPrefix(denom, fullPrefix) {
		return nil, fmt.Errorf("denom prefix(%s) not equal to expected(%s)", denom, fullPrefix)
	}
	contract, err := NewEthAddress(strings.TrimPrefix(denom, fullPrefix))
	switch {
	case err != nil:
		return nil, fmt.Errorf("error(%s) validating ethereum contract address", err)
	case len(denom) != GravityDenomLen:
		return nil, fmt.Errorf("len(denom)(%d) not equal to GravityDenomLen(%d)", len(denom), GravityDenomLen)
	default:
		return contract, nil
	}
}
