use lee_core::account::AccountId;

// Local-test identity only. Select the deployment authority before rebuilding the guest.
pub const INITIAL_EMERGENCY_AUTHORITY: AccountId = AccountId::new([
    216, 100, 77, 141, 182, 125, 254, 222, 181, 57, 217, 151, 160, 222, 203, 213, 19, 59, 0, 210,
    95, 118, 106, 204, 174, 158, 49, 31, 137, 155, 173, 78,
]);
