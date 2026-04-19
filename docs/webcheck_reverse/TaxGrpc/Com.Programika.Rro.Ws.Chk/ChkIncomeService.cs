using System;
using System.Threading;
using System.Threading.Tasks;
using Google.Protobuf;
using Google.Protobuf.Reflection;
using Grpc.Core;

namespace Com.Programika.Rro.Ws.Chk;

public static class ChkIncomeService
{
	[BindServiceMethod(typeof(ChkIncomeService), "BindService")]
	public abstract class ChkIncomeServiceBase
	{
		public virtual Task<CheckResponse> sendChk(Check request, ServerCallContext context)
		{
			throw new RpcException(new Status(StatusCode.Unimplemented, ""));
		}

		public virtual Task<CheckResponse> sendChkV2(Check request, ServerCallContext context)
		{
			throw new RpcException(new Status(StatusCode.Unimplemented, ""));
		}

		public virtual Task<CheckResponse> lastChk(CheckRequest request, ServerCallContext context)
		{
			throw new RpcException(new Status(StatusCode.Unimplemented, ""));
		}

		public virtual Task<CheckResponse> ping(Check request, ServerCallContext context)
		{
			throw new RpcException(new Status(StatusCode.Unimplemented, ""));
		}

		public virtual Task<CheckResponse> delLastChk(CheckRequest request, ServerCallContext context)
		{
			throw new RpcException(new Status(StatusCode.Unimplemented, ""));
		}

		public virtual Task<CheckResponse> delLastChkId(CheckRequestId request, ServerCallContext context)
		{
			throw new RpcException(new Status(StatusCode.Unimplemented, ""));
		}

		public virtual Task<StatusResponse> statusRro(CheckRequest request, ServerCallContext context)
		{
			throw new RpcException(new Status(StatusCode.Unimplemented, ""));
		}

		public virtual Task<RroInfoResponse> infoRro(CheckRequest request, ServerCallContext context)
		{
			throw new RpcException(new Status(StatusCode.Unimplemented, ""));
		}
	}

	public class ChkIncomeServiceClient : ClientBase<ChkIncomeServiceClient>
	{
		public ChkIncomeServiceClient(ChannelBase channel)
			: base(channel)
		{
		}

		public ChkIncomeServiceClient(CallInvoker callInvoker)
			: base(callInvoker)
		{
		}

		protected ChkIncomeServiceClient()
		{
		}

		protected ChkIncomeServiceClient(ClientBaseConfiguration configuration)
			: base(configuration)
		{
		}

		public virtual CheckResponse sendChk(Check request, Metadata headers = null, DateTime? deadline = null, CancellationToken cancellationToken = default(CancellationToken))
		{
			return sendChk(request, new CallOptions(headers, deadline, cancellationToken));
		}

		public virtual CheckResponse sendChk(Check request, CallOptions options)
		{
			return base.CallInvoker.BlockingUnaryCall(__Method_sendChk, null, options, request);
		}

		public virtual AsyncUnaryCall<CheckResponse> sendChkAsync(Check request, Metadata headers = null, DateTime? deadline = null, CancellationToken cancellationToken = default(CancellationToken))
		{
			return sendChkAsync(request, new CallOptions(headers, deadline, cancellationToken));
		}

		public virtual AsyncUnaryCall<CheckResponse> sendChkAsync(Check request, CallOptions options)
		{
			return base.CallInvoker.AsyncUnaryCall(__Method_sendChk, null, options, request);
		}

		public virtual CheckResponse sendChkV2(Check request, Metadata headers = null, DateTime? deadline = null, CancellationToken cancellationToken = default(CancellationToken))
		{
			return sendChkV2(request, new CallOptions(headers, deadline, cancellationToken));
		}

		public virtual CheckResponse sendChkV2(Check request, CallOptions options)
		{
			return base.CallInvoker.BlockingUnaryCall(__Method_sendChkV2, null, options, request);
		}

		public virtual AsyncUnaryCall<CheckResponse> sendChkV2Async(Check request, Metadata headers = null, DateTime? deadline = null, CancellationToken cancellationToken = default(CancellationToken))
		{
			return sendChkV2Async(request, new CallOptions(headers, deadline, cancellationToken));
		}

		public virtual AsyncUnaryCall<CheckResponse> sendChkV2Async(Check request, CallOptions options)
		{
			return base.CallInvoker.AsyncUnaryCall(__Method_sendChkV2, null, options, request);
		}

		public virtual CheckResponse lastChk(CheckRequest request, Metadata headers = null, DateTime? deadline = null, CancellationToken cancellationToken = default(CancellationToken))
		{
			return lastChk(request, new CallOptions(headers, deadline, cancellationToken));
		}

		public virtual CheckResponse lastChk(CheckRequest request, CallOptions options)
		{
			return base.CallInvoker.BlockingUnaryCall(__Method_lastChk, null, options, request);
		}

		public virtual AsyncUnaryCall<CheckResponse> lastChkAsync(CheckRequest request, Metadata headers = null, DateTime? deadline = null, CancellationToken cancellationToken = default(CancellationToken))
		{
			return lastChkAsync(request, new CallOptions(headers, deadline, cancellationToken));
		}

		public virtual AsyncUnaryCall<CheckResponse> lastChkAsync(CheckRequest request, CallOptions options)
		{
			return base.CallInvoker.AsyncUnaryCall(__Method_lastChk, null, options, request);
		}

		public virtual CheckResponse ping(Check request, Metadata headers = null, DateTime? deadline = null, CancellationToken cancellationToken = default(CancellationToken))
		{
			return ping(request, new CallOptions(headers, deadline, cancellationToken));
		}

		public virtual CheckResponse ping(Check request, CallOptions options)
		{
			return base.CallInvoker.BlockingUnaryCall(__Method_ping, null, options, request);
		}

		public virtual AsyncUnaryCall<CheckResponse> pingAsync(Check request, Metadata headers = null, DateTime? deadline = null, CancellationToken cancellationToken = default(CancellationToken))
		{
			return pingAsync(request, new CallOptions(headers, deadline, cancellationToken));
		}

		public virtual AsyncUnaryCall<CheckResponse> pingAsync(Check request, CallOptions options)
		{
			return base.CallInvoker.AsyncUnaryCall(__Method_ping, null, options, request);
		}

		public virtual CheckResponse delLastChk(CheckRequest request, Metadata headers = null, DateTime? deadline = null, CancellationToken cancellationToken = default(CancellationToken))
		{
			return delLastChk(request, new CallOptions(headers, deadline, cancellationToken));
		}

		public virtual CheckResponse delLastChk(CheckRequest request, CallOptions options)
		{
			return base.CallInvoker.BlockingUnaryCall(__Method_delLastChk, null, options, request);
		}

		public virtual AsyncUnaryCall<CheckResponse> delLastChkAsync(CheckRequest request, Metadata headers = null, DateTime? deadline = null, CancellationToken cancellationToken = default(CancellationToken))
		{
			return delLastChkAsync(request, new CallOptions(headers, deadline, cancellationToken));
		}

		public virtual AsyncUnaryCall<CheckResponse> delLastChkAsync(CheckRequest request, CallOptions options)
		{
			return base.CallInvoker.AsyncUnaryCall(__Method_delLastChk, null, options, request);
		}

		public virtual CheckResponse delLastChkId(CheckRequestId request, Metadata headers = null, DateTime? deadline = null, CancellationToken cancellationToken = default(CancellationToken))
		{
			return delLastChkId(request, new CallOptions(headers, deadline, cancellationToken));
		}

		public virtual CheckResponse delLastChkId(CheckRequestId request, CallOptions options)
		{
			return base.CallInvoker.BlockingUnaryCall(__Method_delLastChkId, null, options, request);
		}

		public virtual AsyncUnaryCall<CheckResponse> delLastChkIdAsync(CheckRequestId request, Metadata headers = null, DateTime? deadline = null, CancellationToken cancellationToken = default(CancellationToken))
		{
			return delLastChkIdAsync(request, new CallOptions(headers, deadline, cancellationToken));
		}

		public virtual AsyncUnaryCall<CheckResponse> delLastChkIdAsync(CheckRequestId request, CallOptions options)
		{
			return base.CallInvoker.AsyncUnaryCall(__Method_delLastChkId, null, options, request);
		}

		public virtual StatusResponse statusRro(CheckRequest request, Metadata headers = null, DateTime? deadline = null, CancellationToken cancellationToken = default(CancellationToken))
		{
			return statusRro(request, new CallOptions(headers, deadline, cancellationToken));
		}

		public virtual StatusResponse statusRro(CheckRequest request, CallOptions options)
		{
			return base.CallInvoker.BlockingUnaryCall(__Method_statusRro, null, options, request);
		}

		public virtual AsyncUnaryCall<StatusResponse> statusRroAsync(CheckRequest request, Metadata headers = null, DateTime? deadline = null, CancellationToken cancellationToken = default(CancellationToken))
		{
			return statusRroAsync(request, new CallOptions(headers, deadline, cancellationToken));
		}

		public virtual AsyncUnaryCall<StatusResponse> statusRroAsync(CheckRequest request, CallOptions options)
		{
			return base.CallInvoker.AsyncUnaryCall(__Method_statusRro, null, options, request);
		}

		public virtual RroInfoResponse infoRro(CheckRequest request, Metadata headers = null, DateTime? deadline = null, CancellationToken cancellationToken = default(CancellationToken))
		{
			return infoRro(request, new CallOptions(headers, deadline, cancellationToken));
		}

		public virtual RroInfoResponse infoRro(CheckRequest request, CallOptions options)
		{
			return base.CallInvoker.BlockingUnaryCall(__Method_infoRro, null, options, request);
		}

		public virtual AsyncUnaryCall<RroInfoResponse> infoRroAsync(CheckRequest request, Metadata headers = null, DateTime? deadline = null, CancellationToken cancellationToken = default(CancellationToken))
		{
			return infoRroAsync(request, new CallOptions(headers, deadline, cancellationToken));
		}

		public virtual AsyncUnaryCall<RroInfoResponse> infoRroAsync(CheckRequest request, CallOptions options)
		{
			return base.CallInvoker.AsyncUnaryCall(__Method_infoRro, null, options, request);
		}

		protected override ChkIncomeServiceClient NewInstance(ClientBaseConfiguration configuration)
		{
			return new ChkIncomeServiceClient(configuration);
		}
	}

	private static readonly string __ServiceName = "com.programika.rro.ws.chk.ChkIncomeService";

	private static readonly Marshaller<Check> __Marshaller_com_programika_rro_ws_chk_Check = Marshallers.Create((Check arg) => arg.ToByteArray(), Check.Parser.ParseFrom);

	private static readonly Marshaller<CheckResponse> __Marshaller_com_programika_rro_ws_chk_CheckResponse = Marshallers.Create((CheckResponse arg) => arg.ToByteArray(), CheckResponse.Parser.ParseFrom);

	private static readonly Marshaller<CheckRequest> __Marshaller_com_programika_rro_ws_chk_CheckRequest = Marshallers.Create((CheckRequest arg) => arg.ToByteArray(), CheckRequest.Parser.ParseFrom);

	private static readonly Marshaller<CheckRequestId> __Marshaller_com_programika_rro_ws_chk_CheckRequestId = Marshallers.Create((CheckRequestId arg) => arg.ToByteArray(), CheckRequestId.Parser.ParseFrom);

	private static readonly Marshaller<StatusResponse> __Marshaller_com_programika_rro_ws_chk_StatusResponse = Marshallers.Create((StatusResponse arg) => arg.ToByteArray(), StatusResponse.Parser.ParseFrom);

	private static readonly Marshaller<RroInfoResponse> __Marshaller_com_programika_rro_ws_chk_RroInfoResponse = Marshallers.Create((RroInfoResponse arg) => arg.ToByteArray(), RroInfoResponse.Parser.ParseFrom);

	private static readonly Method<Check, CheckResponse> __Method_sendChk = new Method<Check, CheckResponse>(MethodType.Unary, __ServiceName, "sendChk", __Marshaller_com_programika_rro_ws_chk_Check, __Marshaller_com_programika_rro_ws_chk_CheckResponse);

	private static readonly Method<Check, CheckResponse> __Method_sendChkV2 = new Method<Check, CheckResponse>(MethodType.Unary, __ServiceName, "sendChkV2", __Marshaller_com_programika_rro_ws_chk_Check, __Marshaller_com_programika_rro_ws_chk_CheckResponse);

	private static readonly Method<CheckRequest, CheckResponse> __Method_lastChk = new Method<CheckRequest, CheckResponse>(MethodType.Unary, __ServiceName, "lastChk", __Marshaller_com_programika_rro_ws_chk_CheckRequest, __Marshaller_com_programika_rro_ws_chk_CheckResponse);

	private static readonly Method<Check, CheckResponse> __Method_ping = new Method<Check, CheckResponse>(MethodType.Unary, __ServiceName, "ping", __Marshaller_com_programika_rro_ws_chk_Check, __Marshaller_com_programika_rro_ws_chk_CheckResponse);

	private static readonly Method<CheckRequest, CheckResponse> __Method_delLastChk = new Method<CheckRequest, CheckResponse>(MethodType.Unary, __ServiceName, "delLastChk", __Marshaller_com_programika_rro_ws_chk_CheckRequest, __Marshaller_com_programika_rro_ws_chk_CheckResponse);

	private static readonly Method<CheckRequestId, CheckResponse> __Method_delLastChkId = new Method<CheckRequestId, CheckResponse>(MethodType.Unary, __ServiceName, "delLastChkId", __Marshaller_com_programika_rro_ws_chk_CheckRequestId, __Marshaller_com_programika_rro_ws_chk_CheckResponse);

	private static readonly Method<CheckRequest, StatusResponse> __Method_statusRro = new Method<CheckRequest, StatusResponse>(MethodType.Unary, __ServiceName, "statusRro", __Marshaller_com_programika_rro_ws_chk_CheckRequest, __Marshaller_com_programika_rro_ws_chk_StatusResponse);

	private static readonly Method<CheckRequest, RroInfoResponse> __Method_infoRro = new Method<CheckRequest, RroInfoResponse>(MethodType.Unary, __ServiceName, "infoRro", __Marshaller_com_programika_rro_ws_chk_CheckRequest, __Marshaller_com_programika_rro_ws_chk_RroInfoResponse);

	public static ServiceDescriptor Descriptor => GreetReflection.Descriptor.Services[0];

	public static ServerServiceDefinition BindService(ChkIncomeServiceBase serviceImpl)
	{
		return ServerServiceDefinition.CreateBuilder().AddMethod(__Method_sendChk, serviceImpl.sendChk).AddMethod(__Method_sendChkV2, serviceImpl.sendChkV2)
			.AddMethod(__Method_lastChk, serviceImpl.lastChk)
			.AddMethod(__Method_ping, serviceImpl.ping)
			.AddMethod(__Method_delLastChk, serviceImpl.delLastChk)
			.AddMethod(__Method_delLastChkId, serviceImpl.delLastChkId)
			.AddMethod(__Method_statusRro, serviceImpl.statusRro)
			.AddMethod(__Method_infoRro, serviceImpl.infoRro)
			.Build();
	}

	public static void BindService(ServiceBinderBase serviceBinder, ChkIncomeServiceBase serviceImpl)
	{
		serviceBinder.AddMethod(__Method_sendChk, (serviceImpl == null) ? null : new UnaryServerMethod<Check, CheckResponse>(serviceImpl.sendChk));
		serviceBinder.AddMethod(__Method_sendChkV2, (serviceImpl == null) ? null : new UnaryServerMethod<Check, CheckResponse>(serviceImpl.sendChkV2));
		serviceBinder.AddMethod(__Method_lastChk, (serviceImpl == null) ? null : new UnaryServerMethod<CheckRequest, CheckResponse>(serviceImpl.lastChk));
		serviceBinder.AddMethod(__Method_ping, (serviceImpl == null) ? null : new UnaryServerMethod<Check, CheckResponse>(serviceImpl.ping));
		serviceBinder.AddMethod(__Method_delLastChk, (serviceImpl == null) ? null : new UnaryServerMethod<CheckRequest, CheckResponse>(serviceImpl.delLastChk));
		serviceBinder.AddMethod(__Method_delLastChkId, (serviceImpl == null) ? null : new UnaryServerMethod<CheckRequestId, CheckResponse>(serviceImpl.delLastChkId));
		serviceBinder.AddMethod(__Method_statusRro, (serviceImpl == null) ? null : new UnaryServerMethod<CheckRequest, StatusResponse>(serviceImpl.statusRro));
		serviceBinder.AddMethod(__Method_infoRro, (serviceImpl == null) ? null : new UnaryServerMethod<CheckRequest, RroInfoResponse>(serviceImpl.infoRro));
	}
}
