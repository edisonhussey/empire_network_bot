/// Pure rules for the colour-coded maps: whose colour an item takes. No DOM, no
/// network (see shared-layers.test.js).

/// Which account a threat belongs to, and how sure that is: the account whose player
/// is its target (2), else the one whose player sent it (1), else only the account
/// that happened to record it, as an alliance-wide sighting (0).
export function involvement(row, recorder, accounts) {
  const byPlayer = (playerId) => accounts.find((account) => account.player_id != null && account.player_id === playerId);
  const target = row.target_id != null ? byPlayer(row.target_id) : null;
  if (target) return { owner: target.account_id, rank: 2 };
  const sender = row.sender_id != null ? byPlayer(row.sender_id) : null;
  if (sender) return { owner: sender.account_id, rank: 1 };
  return { owner: recorder, rank: 0 };
}

/// Every active threat across the accounts, once each. Accounts in one alliance record
/// the same movement, so a MID seen by several is drawn once, in the colour of the
/// account it concerns most.
export function mergedThreats(accounts) {
  const byMid = new Map();
  for (const account of accounts) {
    for (const row of account.pvp || []) {
      const { owner, rank } = involvement(row, account.account_id, accounts);
      const held = byMid.get(row.mid);
      if (!held || rank > held.rank) byMid.set(row.mid, { row, owner, rank });
    }
  }
  return [...byMid.values()];
}

/// The accounts that should be drawn on a map of `kingdomId`: those with anything
/// there (a castle, an army, a threat or a stationed castle), the viewed one always.
export function accountsToDraw(accounts) {
  return accounts.filter((account) => account.viewed || account.castle || (account.movements || []).length || (account.pvp || []).length || (account.station || []).length);
}
