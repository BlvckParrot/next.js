import * as React from 'react'
import { getMessagesAsync } from 'pkg-with-async-import'

export default async function Page() {
  const messages = await getMessagesAsync()
  return (
    <main>
      <p>{messages.title}</p>
    </main>
  )
}
