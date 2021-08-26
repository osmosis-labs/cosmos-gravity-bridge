// +build gofuzzbeta

package keeper

import (
	"encoding/binary"
	"fmt"
	"math"
	"math/big"
	"testing"

	sdk "github.com/cosmos/cosmos-sdk/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/althea-net/cosmos-gravity-bridge/module/x/gravity/types"
)

func TestAddToOutgoingPool(t *testing.T) {
	input := CreateTestEnv(t)
	ctx := input.Context
	var (
		mySender, _         = sdk.AccAddressFromBech32("cosmos1ahx7f8wyertuus9r20284ej0asrs085case3kn")
		myReceiver          = "0xd041c41EA1bf0F006ADBb6d2c9ef9D425dE5eaD7"
		myTokenContractAddr = "0x429881672B9AE42b8EbA0E26cD9C73711b891Ca5"
	)
	// mint some voucher first
	allVouchers := sdk.Coins{types.NewERC20Token(99999, myTokenContractAddr).GravityCoin()}
	err := input.BankKeeper.MintCoins(ctx, types.ModuleName, allVouchers)
	require.NoError(t, err)

	// set senders balance
	input.AccountKeeper.NewAccountWithAddress(ctx, mySender)
	err = input.BankKeeper.SetBalances(ctx, mySender, allVouchers)
	require.NoError(t, err)

	// when
	for i, v := range []uint64{2, 3, 2, 1} {
		amount := types.NewERC20Token(uint64(i+100), myTokenContractAddr).GravityCoin()
		fee := types.NewERC20Token(v, myTokenContractAddr).GravityCoin()
		r, err := input.GravityKeeper.AddToOutgoingPool(ctx, mySender, myReceiver, amount, fee)
		require.NoError(t, err)
		t.Logf("___ response: %#v", r)
		// Should create:
		// 1: amount 100, fee 2
		// 2: amount 101, fee 3
		// 3: amount 102, fee 2
		// 4: amount 103, fee 1

	}
	// then
	got := input.GravityKeeper.GetUnbatchedTransactionsByContract(ctx, myTokenContractAddr)

	exp := []*types.OutgoingTransferTx{
		{
			Id:          2,
			Erc20Fee:    types.NewERC20Token(3, myTokenContractAddr),
			Sender:      mySender.String(),
			DestAddress: myReceiver,
			Erc20Token:  types.NewERC20Token(101, myTokenContractAddr),
		},
		{
			Id:          3,
			Erc20Fee:    types.NewERC20Token(2, myTokenContractAddr),
			Sender:      mySender.String(),
			DestAddress: myReceiver,
			Erc20Token:  types.NewERC20Token(102, myTokenContractAddr),
		},
		{
			Id:          1,
			Erc20Fee:    types.NewERC20Token(2, myTokenContractAddr),
			Sender:      mySender.String(),
			DestAddress: myReceiver,
			Erc20Token:  types.NewERC20Token(100, myTokenContractAddr),
		},
		{
			Id:          4,
			Erc20Fee:    types.NewERC20Token(1, myTokenContractAddr),
			Sender:      mySender.String(),
			DestAddress: myReceiver,
			Erc20Token:  types.NewERC20Token(103, myTokenContractAddr),
		},
	}
	assert.Equal(t, exp, got)
}

func createSeedUint64ByteArrayWithValue(numElts int, value uint64) []byte {
	numBytes := numElts * 64
	arr := make([]byte, numBytes)
	for i := 0; i < numElts; i++ {
		binary.PutUvarint(arr[64*i:64*(i+1)], value)
	}
	return arr
}

func deserializeByteArrayToUint64Array(bytes []byte, numElts int) []uint64 {
	uints := make([]uint64, numElts)

	for j := 0; j < numElts; j++ {
		uints[j] = binary.BigEndian.Uint64(bytes[64*j : 64*(j+1)])
	}
	return uints
}

func FuzzAddToOutgoingPool(f *testing.F) {
	numInputs := 6
	ones := createSeedUint64ByteArrayWithValue(numInputs, uint64(1))
	oneHundreds := createSeedUint64ByteArrayWithValue(numInputs, uint64(100))

	f.Add(ones, oneHundreds, "0000000000000000000000000000000000000000")
	f.Fuzz(func(t *testing.T, feez []byte, amountz []byte, contractAddr string) {
		if types.ValidateEthAddress(contractAddr) != nil {
			t.Skip()
		}
		fees := deserializeByteArrayToUint64Array(feez, numInputs)
		amounts := deserializeByteArrayToUint64Array(amountz, numInputs)
		for j := 0; j < numInputs; j++ {
			fees[j] = binary.BigEndian.Uint64(feez[64*j : 64*(j+1)])
			amounts[j] = binary.BigEndian.Uint64(amountz[64*j : 64*(j+1)])
		}

		input := CreateTestEnv(t)
		ctx := input.Context
		var (
			mySender, _         = sdk.AccAddressFromBech32("cosmos1ahx7f8wyertuus9r20284ej0asrs085case3kn")
			myReceiver          = "0xd041c41EA1bf0F006ADBb6d2c9ef9D425dE5eaD7"
			myTokenContractAddr = "0x" + contractAddr
		)

		// mint some voucher first
		var balance uint64 = math.MaxUint64
		allVouchers := sdk.Coins{types.NewERC20Token(balance, myTokenContractAddr).GravityCoin()}
		err := input.BankKeeper.MintCoins(ctx, types.ModuleName, allVouchers)
		if err != nil {
			t.Skip()
		}

		// set senders balance
		input.AccountKeeper.NewAccountWithAddress(ctx, mySender)
		err = input.BankKeeper.SetBalances(ctx, mySender, allVouchers)
		if err != nil {
			t.Skip()
		}

		transacts := make([]*types.OutgoingTransferTx, len(fees))
		// create transactions
		for i, _ := range fees {
			txAmt := amounts[i]
			txFee := fees[i]
			if uint64(txAmt+txFee) >= balance {
				txAmt = uint64(balance / 2)
				txFee = uint64(txAmt / 2)
			}
			amount := types.NewERC20Token(txAmt, myTokenContractAddr).GravityCoin()
			fee := types.NewERC20Token(txFee, myTokenContractAddr).GravityCoin()
			transacts[i] = &types.OutgoingTransferTx{
				Id:          uint64(i + 1),
				Sender:      mySender.String(),
				DestAddress: myReceiver,
				Erc20Token:  types.NewSDKIntERC20Token(amount.Amount, amount.Denom),
				Erc20Fee:    types.NewSDKIntERC20Token(fee.Amount, fee.Denom),
			}
			r, err := input.GravityKeeper.AddToOutgoingPool(ctx, mySender, myReceiver, amount, fee)
			balance = balance - uint64(txAmt+txFee)
			require.NoError(t, err)
			t.Logf("___ response: %#v", r)
			// Should create:
			// 1: amount 100, fee 2
			// 2: amount 101, fee 3
			// 3: amount 102, fee 2
			// 4: amount 103, fee 1

		}

		got := input.GravityKeeper.GetUnbatchedTransactionsByContract(ctx, myTokenContractAddr)
		if len(got) != len(fees) {
			t.Fatal(fmt.Errorf("generated transactions do not match ones received\nexpected: %v\nreceived: %v", transacts, got))
		}
	})

}

func TestTotalBatchFeeInPool(t *testing.T) {
	input := CreateTestEnv(t)
	ctx := input.Context

	// token1
	var (
		mySender, _         = sdk.AccAddressFromBech32("cosmos1ahx7f8wyertuus9r20284ej0asrs085case3kn")
		myReceiver          = "0xd041c41EA1bf0F006ADBb6d2c9ef9D425dE5eaD7"
		myTokenContractAddr = "0x429881672B9AE42b8EbA0E26cD9C73711b891Ca5"
	)
	// mint some voucher first
	allVouchers := sdk.Coins{types.NewERC20Token(99999, myTokenContractAddr).GravityCoin()}
	err := input.BankKeeper.MintCoins(ctx, types.ModuleName, allVouchers)
	require.NoError(t, err)

	// set senders balance
	input.AccountKeeper.NewAccountWithAddress(ctx, mySender)
	err = input.BankKeeper.SetBalances(ctx, mySender, allVouchers)
	require.NoError(t, err)

	// create outgoing pool
	for i, v := range []uint64{2, 3, 2, 1} {
		amount := types.NewERC20Token(uint64(i+100), myTokenContractAddr).GravityCoin()
		fee := types.NewERC20Token(v, myTokenContractAddr).GravityCoin()
		r, err2 := input.GravityKeeper.AddToOutgoingPool(ctx, mySender, myReceiver, amount, fee)
		require.NoError(t, err2)
		t.Logf("___ response: %#v", r)
	}

	// token 2 - Only top 100
	var (
		myToken2ContractAddr = "0x7D1AfA7B718fb893dB30A3aBc0Cfc608AaCfeBB0"
	)
	// mint some voucher first
	allVouchers = sdk.Coins{types.NewERC20Token(18446744073709551615, myToken2ContractAddr).GravityCoin()}
	err = input.BankKeeper.MintCoins(ctx, types.ModuleName, allVouchers)
	require.NoError(t, err)

	// set senders balance
	input.AccountKeeper.NewAccountWithAddress(ctx, mySender)
	err = input.BankKeeper.SetBalances(ctx, mySender, allVouchers)
	require.NoError(t, err)

	// Add

	// create outgoing pool
	for i := 0; i < 110; i++ {
		amount := types.NewERC20Token(uint64(i+100), myToken2ContractAddr).GravityCoin()
		fee := types.NewERC20Token(uint64(5), myToken2ContractAddr).GravityCoin()
		r, err := input.GravityKeeper.AddToOutgoingPool(ctx, mySender, myReceiver, amount, fee)
		require.NoError(t, err)
		t.Logf("___ response: %#v", r)
	}

	batchFees := input.GravityKeeper.GetAllBatchFees(ctx, OutgoingTxBatchSize)
	/*
		tokenFeeMap should be
		map[0x429881672B9AE42b8EbA0E26cD9C73711b891Ca5:8 0x7D1AfA7B718fb893dB30A3aBc0Cfc608AaCfeBB0:500]
		**/
	assert.Equal(t, batchFees[0].TotalFees.BigInt(), big.NewInt(int64(8)))
	assert.Equal(t, batchFees[1].TotalFees.BigInt(), big.NewInt(int64(500)))

}
