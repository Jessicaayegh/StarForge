- closes #730
- closes #794

### Advancements Included:
- **Wallet Address Book**: Developed a verified `AddressBook` module capable of logging recognized wallet addresses and issuing dynamic safety warnings during external transfers to unverified or unknown endpoints.
- **CI Pipeline**: Implemented an automated `size_budget.sh` script to track the binary footprint of release artifacts, throwing CI failures if the bundle size exceeds the 50MB budget constraints.
