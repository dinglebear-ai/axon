package com.axon.app.data.remote

import okhttp3.ConnectionPool
import okhttp3.Dispatcher
import okhttp3.OkHttpClient
import java.util.concurrent.TimeUnit

private const val CONNECT_TIMEOUT_SECONDS = 10L
private const val READ_TIMEOUT_SECONDS = 60L
private const val LONG_READ_TIMEOUT_SECONDS = 300L
private const val STREAM_READ_TIMEOUT_SECONDS = 300L
private const val WRITE_TIMEOUT_SECONDS = 15L

internal class AxonHttpClients {
    private val sharedPool = ConnectionPool(
        maxIdleConnections = 16,
        keepAliveDuration = 5,
        TimeUnit.MINUTES,
    )
    private val sharedDispatcher = Dispatcher().apply { maxRequestsPerHost = 16 }

    val normal: OkHttpClient = OkHttpClient.Builder()
        .connectionPool(sharedPool)
        .dispatcher(sharedDispatcher)
        .connectTimeout(CONNECT_TIMEOUT_SECONDS, TimeUnit.SECONDS)
        .readTimeout(READ_TIMEOUT_SECONDS, TimeUnit.SECONDS)
        .writeTimeout(WRITE_TIMEOUT_SECONDS, TimeUnit.SECONDS)
        .build()

    val longRead: OkHttpClient = normal.newBuilder()
        .readTimeout(LONG_READ_TIMEOUT_SECONDS, TimeUnit.SECONDS)
        .build()

    val stream: OkHttpClient = normal.newBuilder()
        .readTimeout(STREAM_READ_TIMEOUT_SECONDS, TimeUnit.SECONDS)
        .build()
}
