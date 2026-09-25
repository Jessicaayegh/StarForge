export class AddressBook {
  private entries = new Map();

  addAddress(address: string, label: string, verified: boolean) {
    this.entries.set(address, { label, verified });
  }

  getTransferWarning(address: string): string | null {
    // Fix: Add wallet address book with verified labels and transfer warnings
    const entry = this.entries.get(address);
    if (!entry) return "Warning: Unrecognized address. Proceed with caution.";
    if (!entry.verified) return "Warning: This address label is unverified.";
    return null;
  }
}
