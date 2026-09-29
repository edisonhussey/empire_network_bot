import unittest

from bot.account_subscription import subscribe
from bot.accounts import ACCOUNTS, Account, Pingpoko, Ventrilo
from bot.event.sand.config import SAND_KUNAI, SAND_LV35_61_DEATHLY_HORROR, SAND_LV61_CROSSBOW
from bot.subscriptions import subscription_for, subscription_records


class AccountObjectTests(unittest.TestCase):
    def test_named_accounts_are_typed_registry_objects(self):
        self.assertIsInstance(Ventrilo, Account)
        self.assertIsInstance(Pingpoko, Account)
        self.assertEqual(ACCOUNTS[Ventrilo.key], Ventrilo)
        self.assertEqual(Ventrilo.python_name, "Ventrilo")
        self.assertEqual(Pingpoko.python_name, "Pingpoko")

    def test_subscription_syntax_preserves_task_order(self):
        subscription = subscribe(Ventrilo)(
            SAND_LV61_CROSSBOW,
            SAND_LV35_61_DEATHLY_HORROR,
        )
        self.assertEqual(
            tuple(task.name for task in subscription.tasks),
            (SAND_LV61_CROSSBOW.name, SAND_LV35_61_DEATHLY_HORROR.name),
        )

    def test_declarative_subscription_is_queryable_by_account(self):
        subscription = subscription_for(Ventrilo)
        self.assertIsNotNone(subscription)
        self.assertEqual(subscription.account, Ventrilo)
        self.assertEqual(
            tuple(task.name for task in subscription.tasks),
            ("sand_rbc_level_61_crossbow", "sand_kunai"),
        )
        self.assertEqual(subscription_for("VENTRILO"), subscription)

    def test_sand_kunai_is_public_and_subscribed(self):
        from bot.tasks import SAND_KUNAI as public_sand_kunai

        self.assertIs(public_sand_kunai, SAND_KUNAI)
        self.assertIn(SAND_KUNAI, subscription_for(Ventrilo).tasks)

    def test_registry_has_mapping_semantics_for_unknown_account(self):
        with self.assertRaises(KeyError):
            ACCOUNTS["not-a-configured-account"]

    def test_public_records_do_not_expose_credentials(self):
        records = subscription_records()
        ventrilo = next(record for record in records if record["account"]["key"] == "ventrilo")
        self.assertNotIn("password", {key.lower() for key in ventrilo["account"]})
        self.assertNotIn("credentials", ventrilo["account"])
        self.assertEqual(ventrilo["account"]["has_credentials"], True)


if __name__ == "__main__":
    unittest.main()
