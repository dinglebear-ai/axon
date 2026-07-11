import '../src/styles/styles.css';
import type { Metadata } from 'next';

export const metadata: Metadata = {
  metadataBase: new URL(process.env.NEXT_PUBLIC_AXON_PUBLIC_URL ?? 'http://localhost:3000'),
  title: 'Axon Admin',
  description: 'Axon setup and operations panel',
  icons: {
    icon: [
      { url: '/assets/png/axon-icon-32.png', sizes: '32x32', type: 'image/png' },
      { url: '/assets/png/axon-icon-192.png', sizes: '192x192', type: 'image/png' }
    ],
    apple: [{ url: '/assets/png/axon-icon-180.png', sizes: '180x180', type: 'image/png' }]
  },
  openGraph: {
    title: 'Axon Admin',
    description: 'Axon setup and operations panel',
    images: [{ url: '/assets/png/axon-og-1200x630.png', width: 1200, height: 630, alt: 'Axon' }]
  }
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className="dark">
      <body>{children}</body>
    </html>
  );
}
