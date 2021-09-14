package keeper_test

import (
	"github.com/althea-net/cosmos-gravity-bridge/module/x/bech32ibc/types"
	sdk "github.com/cosmos/cosmos-sdk/types"
)

func (suite *KeeperTestSuite) TestHrpIbcRecords() {
	suite.SetupTest()

	// check genesis native hrp
	resp, err := suite.queryClient.HrpIbcRecords(sdk.WrapSDKContext(suite.ctx), &types.QueryHrpIbcRecordsRequest{})
	suite.Require().NoError(err)
	suite.Require().Len(resp.HrpIbcRecords, 0)
}

func (suite *KeeperTestSuite) TestHrpSourceChannel() {
	suite.SetupTest()

	// check genesis source channel
	resp, err := suite.queryClient.HrpSourceChannel(sdk.WrapSDKContext(suite.ctx), &types.QueryHrpSourceChannelRequest{
		Hrp: "akash",
	})
	suite.Require().Error(err)
	suite.Require().Nil(resp)
}

func (suite *KeeperTestSuite) TestNativeHrp() {
	suite.SetupTest()

	// check genesis native hrp
	resp, err := suite.queryClient.NativeHrp(sdk.WrapSDKContext(suite.ctx), &types.QueryNativeHrpRequest{})
	suite.Require().NoError(err)
	suite.Require().Equal(resp.NativeHrp, "osmo")
}
