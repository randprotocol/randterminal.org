import type { Metadata } from 'next';
import { ZkvmTour } from '@/components/ZkvmTour';

export const metadata: Metadata = {
  title: 'How the zkVM works · Rand Terminal',
  description: 'From Solidity or a Solana program to a verified STARK proof: the transpilers, the RV32IM image, the trace tables, the constraints, the commitment and FRI, step by step.',
};

export default function Page() { return <ZkvmTour />; }
