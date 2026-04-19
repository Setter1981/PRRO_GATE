using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using Com.Programika.Rro.Ws.Chk;
using Google.Protobuf;
using Grpc.Core;

namespace TaxGrpc;

[Guid("2D7D125D-166C-4CF1-A979-7738BC41A7BF")]
[ClassInterface(ClassInterfaceType.None)]
[ComVisible(true)]
public class Client : IClient
{
	private enum CheckAction
	{
		InvalidVersion,
		Check,
		Ping,
		CheckV2
	}

	private Channel _channel;

	private ChkIncomeService.ChkIncomeServiceClient _client;

	private Answer _result = new Answer();

	public void Open(string host)
	{
		if (_channel == null)
		{
			string rootCertificates = File.ReadAllText("C:\\ProgramData\\WebCheck\\prro-tax-gov-ua-chain.pem");
			_channel = new Channel(host, new SslCredentials(rootCertificates));
			_client = new ChkIncomeService.ChkIncomeServiceClient(_channel);
		}
	}

	private void CheckOpen()
	{
		if (_channel == null)
		{
			throw new Exception("gRPC not opened");
		}
	}

	private void ClearResult()
	{
		_result.IdSign = "";
		_result.IdData = "";
		_result.Id = "";
		_result.Message = "";
		_result.Status = 0;
	}

	public Answer Check(int version, string pathf, string fn, long date, int number, byte type, string idCancel, string idOffline, int timeoutMillis)
	{
		return version switch
		{
			1 => CheckInternal(CheckAction.Check, pathf, fn, date, number, type, idCancel, idOffline, timeoutMillis), 
			2 => CheckInternal(CheckAction.CheckV2, pathf, fn, date, number, type, idCancel, idOffline, timeoutMillis), 
			_ => CheckInternal(CheckAction.InvalidVersion, pathf, fn, date, number, type, idCancel, idOffline, timeoutMillis), 
		};
	}

	public Answer Ping(string pathf, string fn, long date, int number, byte type, string idCancel, string idOffline, int timeoutMillis)
	{
		return CheckInternal(CheckAction.Ping, pathf, fn, date, number, type, idCancel, idOffline, timeoutMillis);
	}

	public Answer CheckLast(string pathf, int timeoutMillis)
	{
		try
		{
			ClearResult();
			CheckOpen();
			ByteString rroFnSign = ReadFile(pathf);
			CheckRequest request = new CheckRequest
			{
				RroFnSign = rroFnSign
			};
			CheckResponse grpcRes = _client.lastChk(request, GetOptions(timeoutMillis));
			FillResult(grpcRes);
		}
		catch (Exception ex)
		{
			_channel.ShutdownAsync();
			_result.Message = ex.Message;
		}
		return _result;
	}

	private Answer CheckInternal(CheckAction action, string pathf, string fn, long date, int number, byte type, string idCancel, string idOffline, int timeoutMillis)
	{
		try
		{
			ClearResult();
			CheckOpen();
			ByteString checkSign = ReadFile(pathf);
			Check request = new Check
			{
				DateTime = date,
				CheckSign = checkSign,
				RroFn = fn,
				IdCancel = idCancel,
				IdOffline = idOffline,
				LocalNumber = number,
				CheckType = (Check.Types.Type)type
			};
			CheckResponse grpcRes;
			switch (action)
			{
			case CheckAction.Check:
				grpcRes = _client.sendChk(request, GetOptions(timeoutMillis));
				break;
			case CheckAction.CheckV2:
				grpcRes = _client.sendChkV2(request, GetOptions(timeoutMillis));
				break;
			case CheckAction.Ping:
				grpcRes = _client.ping(request, GetOptions(timeoutMillis));
				break;
			default:
				_result.Message = string.Format("Invalid {0} = {1}", "CheckAction", action);
				return _result;
			}
			FillResult(grpcRes);
		}
		catch (Exception ex)
		{
			_channel.ShutdownAsync();
			_result.Message = ex.Message;
		}
		return _result;
	}

	private CallOptions GetOptions(int timout)
	{
		return new CallOptions(null, DateTime.UtcNow.AddMilliseconds(timout));
	}

	private ByteString ReadFile(string fileName)
	{
		return ByteString.CopyFrom(File.ReadAllText(fileName, Encoding.GetEncoding(1251)), Encoding.GetEncoding(1251));
	}

	private void FillResult(CheckResponse grpcRes)
	{
		_result.Id = grpcRes.Id;
		_result.Status = (int)grpcRes.Status;
		_result.Message = grpcRes.ErrorMessage;
		if (!grpcRes.IdSign.IsEmpty)
		{
			_result.IdSign = grpcRes.IdSign.ToBase64();
		}
		if (!grpcRes.DataSign.IsEmpty)
		{
			_result.IdData = grpcRes.DataSign.ToBase64();
		}
		_channel.ShutdownAsync();
	}
}
