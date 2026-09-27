/** Crypto donation networks. EVM chains share one address; Solana has its own. */

const EVM_WALLET_ADDRESS = "0x02cc164ccb539733102cfe5a613d32895835a048";
const SOLANA_WALLET_ADDRESS = "7j8tNtE8BbKZGAeafE73cSwFE71wLaDXYdejeyJ6EAsL";

export const NETWORKS = [
  { id: "ethereum", assets: "USDT · USDC · ETH", address: EVM_WALLET_ADDRESS },
  { id: "arbitrum", assets: "USDT · USDC · ETH", address: EVM_WALLET_ADDRESS },
  { id: "bnb", assets: "USDT · USDC · BNB", address: EVM_WALLET_ADDRESS },
  { id: "polygon", assets: "USDT · USDC · POL", address: EVM_WALLET_ADDRESS },
  { id: "base", assets: "USDC · ETH", address: EVM_WALLET_ADDRESS },
  { id: "solana", assets: "USDT · USDC · SOL", address: SOLANA_WALLET_ADDRESS },
] as const;

export type Network = (typeof NETWORKS)[number];
export type NetworkId = Network["id"];

/** Simplified network marks in each chain's brand color. */
export function NetworkLogo({ id, size = 20 }: { id: NetworkId; size?: number }) {
  const common = { width: size, height: size, viewBox: "0 0 24 24", "aria-hidden": true } as const;
  switch (id) {
    case "ethereum":
      return (
        <svg {...common}>
          <path fill="#627EEA" d="M12 1.5 5.5 12.2 12 16l6.5-3.8z" />
          <path fill="#627EEA" opacity=".6" d="m12 17.3-6.5-3.8L12 22.5l6.5-9z" />
        </svg>
      );
    case "arbitrum":
      return (
        <svg {...common}>
          <path fill="#28A0F0" d="m12 1.5 9 5.25v10.5L12 22.5 3 17.25V6.75z" />
          <path fill="#fff" d="m12.1 5.7 5.2 10.6h-2.5l-2.7-5.8-2.7 5.8H6.9z" />
        </svg>
      );
    case "bnb":
      return (
        <svg {...common}>
          <path
            fill="#F3BA2F"
            d="m12 3 2.6 2.6L9.3 11 6.7 8.3zm4.1 4.1 2.6 2.6-8.8 8.8-2.6-2.6zM5.6 9.4 8.2 12l-2.6 2.6L3 12zm12.8 0L21 12l-6.6 6.6-2.6-2.6zM12 18.4l2.6 2.6L12 23.6 9.4 21z"
          />
        </svg>
      );
    case "polygon":
      return (
        <svg {...common}>
          <path
            fill="#8247E5"
            d="M16.6 8.3a1.3 1.3 0 0 0-1.2 0l-2.8 1.6-1.9 1.1-2.8 1.6a1.3 1.3 0 0 1-1.2 0l-2.2-1.3A1.2 1.2 0 0 1 3.9 10V7.5a1.2 1.2 0 0 1 .6-1l2.2-1.3a1.3 1.3 0 0 1 1.2 0l2.2 1.3a1.2 1.2 0 0 1 .6 1v1.6l1.9-1.1V6.4a1.2 1.2 0 0 0-.6-1L8 3a1.3 1.3 0 0 0-1.2 0L2.6 5.4a1.2 1.2 0 0 0-.6 1v4.8a1.2 1.2 0 0 0 .6 1l4.2 2.4a1.3 1.3 0 0 0 1.2 0l2.8-1.6 1.9-1.1 2.8-1.6a1.3 1.3 0 0 1 1.2 0l2.2 1.3a1.2 1.2 0 0 1 .6 1v2.5a1.2 1.2 0 0 1-.6 1l-2.2 1.3a1.3 1.3 0 0 1-1.2 0l-2.2-1.3a1.2 1.2 0 0 1-.6-1v-1.6l-1.9 1.1v1.6a1.2 1.2 0 0 0 .6 1l4.2 2.4a1.3 1.3 0 0 0 1.2 0l4.2-2.4a1.2 1.2 0 0 0 .6-1v-4.8a1.2 1.2 0 0 0-.6-1z"
          />
        </svg>
      );
    case "base":
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="10" fill="#0052FF" />
          <path fill="#fff" d="M12 6a6 6 0 1 0 0 12h4v-3h-4a3 3 0 1 1 0-6h4V6z" />
        </svg>
      );
    case "solana":
      return (
        <svg {...common}>
          <path fill="#9945FF" d="M6.2 16.2h15.3l-3.7 3.7H2.5zM6.2 4.1h15.3l-3.7 3.7H2.5zM17.8 10.15H2.5l3.7 3.7h15.3z" />
        </svg>
      );
  }
}
