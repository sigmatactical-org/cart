//! [`CheckoutChoice`].

use sigma_pg::clients::addresses::AddressSummary;

use crate::payments_client::PaymentMethodSummary;

/// One selectable option on the checkout form. Implemented by the address and
/// payment-method summaries so both render through the same select builder.
pub(crate) trait CheckoutChoice {
    fn choice_id(&self) -> &str;
    fn choice_summary(&self) -> String;
    fn is_choice_default(&self) -> bool;
}

impl CheckoutChoice for AddressSummary {
    fn choice_id(&self) -> &str {
        &self.id
    }

    fn choice_summary(&self) -> String {
        self.short_summary()
    }

    fn is_choice_default(&self) -> bool {
        self.is_default
    }
}

impl CheckoutChoice for PaymentMethodSummary {
    fn choice_id(&self) -> &str {
        &self.id
    }

    fn choice_summary(&self) -> String {
        self.short_summary()
    }

    fn is_choice_default(&self) -> bool {
        self.is_default
    }
}
