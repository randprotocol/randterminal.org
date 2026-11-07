import './globals.css';
import type { Metadata, Viewport } from 'next';
import { Geist, Geist_Mono, IBM_Plex_Mono } from 'next/font/google';
import Script from 'next/script';

const sans = Geist({ subsets: ['latin'], weight: ['400', '500', '600'], display: 'swap', variable: '--font-geist-sans' });
// The wordmark keeps the face it shipped in; everything else is Geist.
const mark = IBM_Plex_Mono({ subsets: ['latin'], weight: ['600'], display: 'swap', variable: '--font-plex-mono' });
const mono = Geist_Mono({ subsets: ['latin'], weight: ['400', '500', '600'], display: 'swap', variable: '--font-geist-mono' });

export const metadata: Metadata = {
  title: 'Rand Terminal',
  description: 'Compile Solidity or Solana programs, transpile them to RV32IM, and step through them on the Rand zkVM.',
  metadataBase: new URL('https://randterminal.org'),
};
export const viewport: Viewport = { themeColor: [{ media: '(prefers-color-scheme: light)', color: '#f7f7f6' }, { media: '(prefers-color-scheme: dark)', color: '#0b0b0b' }] };

const THEME = `(function(){var t='system';try{var s=localStorage.getItem('randterminal.theme');if(s==='light'||s==='dark')t=s}catch(e){}document.documentElement.dataset.theme=t})()`;

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" data-theme="system" suppressHydrationWarning className={`${sans.variable} ${mono.variable} ${mark.variable}`}>
      <body>
        <Script id="theme" strategy="beforeInteractive">{THEME}</Script>
        {children}
      </body>
    </html>
  );
}
