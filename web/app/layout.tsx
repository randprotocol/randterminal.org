import './globals.css';
import type { Metadata, Viewport } from 'next';
import { IBM_Plex_Mono, IBM_Plex_Sans } from 'next/font/google';
import Script from 'next/script';

const sans = IBM_Plex_Sans({ subsets: ['latin'], weight: ['400', '500', '600'], display: 'swap', variable: '--font-plex-sans' });
const mono = IBM_Plex_Mono({ subsets: ['latin'], weight: ['400', '500', '600'], display: 'swap', variable: '--font-plex-mono' });

export const metadata: Metadata = {
  title: 'Rand Terminal',
  description: 'Compile Solidity or Solana programs, transpile them to RV32IM, and step through them on the Rand zkVM.',
  metadataBase: new URL('https://randterminal.org'),
};
export const viewport: Viewport = { themeColor: [{ media: '(prefers-color-scheme: light)', color: '#F2F3F7' }, { media: '(prefers-color-scheme: dark)', color: '#0E1220' }] };

const THEME = `(function(){var t='system';try{var s=localStorage.getItem('randterminal.theme');if(s==='light'||s==='dark')t=s}catch(e){}document.documentElement.dataset.theme=t})()`;

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" data-theme="system" suppressHydrationWarning className={`${sans.variable} ${mono.variable}`}>
      <body>
        <Script id="theme" strategy="beforeInteractive">{THEME}</Script>
        {children}
      </body>
    </html>
  );
}
