using System;
using Google.Protobuf.Reflection;

namespace Com.Programika.Rro.Ws.Chk;

public static class GreetReflection
{
	private static FileDescriptor descriptor;

	public static FileDescriptor Descriptor => descriptor;

	static GreetReflection()
	{
		descriptor = FileDescriptor.FromGeneratedCode(Convert.FromBase64String("ChxUYXhHcnBjL2NvbnRyYWN0L0dyZWV0LnByb3RvEhljb20ucHJvZ3JhbWlr" + "YS5ycm8ud3MuY2hrIvEBCgVDaGVjaxIOCgZycm9fZm4YASABKAkSEQoJZGF0" + "ZV90aW1lGAIgASgDEhIKCmNoZWNrX3NpZ24YAyABKAwSFAoMbG9jYWxfbnVt" + "YmVyGAQgASgFEjkKCmNoZWNrX3R5cGUYBSABKA4yJS5jb20ucHJvZ3JhbWlr" + "YS5ycm8ud3MuY2hrLkNoZWNrLlR5cGUSEgoKaWRfb2ZmbGluZRgGIAEoCRIR" + "CglpZF9jYW5jZWwYByABKAkiOQoEVHlwZRILCgdVTktOT1dOEAASBwoDQ0hL" + "EAESCwoHWlJFUE9SVBACEg4KClNFUlZJQ0VDSEsQAyIjCgxDaGVja1JlcXVl" + "c3QSEwoLcnJvX2ZuX3NpZ24YAyABKAwiMQoOQ2hlY2tSZXF1ZXN0SWQSCgoC" + "aWQYASABKAkSEwoLcnJvX2ZuX3NpZ24YAiABKAwipQUKDUNoZWNrUmVzcG9u" + "c2USCgoCaWQYASABKAkSPwoGc3RhdHVzGAIgASgOMi8uY29tLnByb2dyYW1p" + "a2EucnJvLndzLmNoay5DaGVja1Jlc3BvbnNlLlN0YXR1cxIPCgdpZF9zaWdu" + "GAMgASgMEhEKCWRhdGFfc2lnbhgEIAEoDBIVCg1lcnJvcl9tZXNzYWdlGAUg" + "ASgJIosECgZTdGF0dXMSCwoHVU5LTk9XThAAEgYKAk9LEAESGQoMRVJST1Jf" + "VkVSRUZZEP///////////wESGAoLRVJST1JfQ0hFQ0sQ/v//////////ARIX" + "CgpFUlJPUl9TQVZFEP3//////////wESGgoNRVJST1JfVU5LTk9XThD8////" + "//////8BEhcKCkVSUk9SX1RZUEUQ+///////////ARIjChZFUlJPUl9OT1Rf" + "UFJFVl9aUkVQT1JUEPr//////////wESFgoJRVJST1JfWE1MEPn/////////" + "/wESGwoORVJST1JfWE1MX0RBVEUQ+P//////////ARIaCg1FUlJPUl9YTUxf" + "Q0hLEPf//////////wESHgoRRVJST1JfWE1MX1pSRVBPUlQQ9v//////////" + "ARIeChFFUlJPUl9PRkZMSU5FXzE2OBD1//////////8BEiAKE0VSUk9SX0JB" + "RF9IQVNIX1BSRVYQ9P//////////ARIlChhFUlJPUl9OT1RfUkVHSVNURVJF" + "RF9SUk8Q8///////////ARIoChtFUlJPUl9OT1RfUkVHSVNURVJFRF9TSUdO" + "RVIQ8v//////////ARIhChRFUlJPUl9OT1RfT1BFTl9TSElGVBDx////////" + "//8BEh0KEEVSUk9SX09GRkxJTkVfSUQQ8P//////////ASLkAgoOU3RhdHVz" + "UmVzcG9uc2USEgoKb3Blbl9zaGlmdBgBIAEoCBIOCgZvbmxpbmUYAiABKAgS" + "EwoLbGFzdF9zaWduZXIYAyABKAkSQAoGc3RhdHVzGAQgASgOMjAuY29tLnBy" + "b2dyYW1pa2EucnJvLndzLmNoay5TdGF0dXNSZXNwb25zZS5TdGF0dXMSFQoN" + "ZXJyb3JfbWVzc2FnZRgFIAEoCSK/AQoGU3RhdHVzEgsKB1VOS05PV04QABIG" + "CgJPSxABEhkKDEVSUk9SX1ZFUkVGWRD///////////8BEhgKC0VSUk9SX0NI" + "RUNLEP7//////////wESGgoNRVJST1JfVU5LTk9XThD8//////////8BEiUK" + "GEVSUk9SX05PVF9SRUdJU1RFUkVEX1JSTxDz//////////8BEigKG0VSUk9S" + "X05PVF9SRUdJU1RFUkVEX1NJR05FUhDy//////////8BIpwFCg9Scm9JbmZv" + "UmVzcG9uc2USQQoGc3RhdHVzGAEgASgOMjEuY29tLnByb2dyYW1pa2EucnJv" + "LndzLmNoay5Scm9JbmZvUmVzcG9uc2UuU3RhdHVzEhIKCnN0YXR1c19ycm8Y" + "AiABKAUSEgoKb3Blbl9zaGlmdBgDIAEoCBIOCgZvbmxpbmUYBCABKAgSEwoL" + "bGFzdF9zaWduZXIYBSABKAkSDAoEbmFtZRgGIAEoCRIPCgduYW1lX3RvGAcg" + "ASgJEgwKBGFkZHIYCCABKAkSEgoKc2luZ2xlX3RheBgJIAEoCBIXCg9vZmZs" + "aW5lX2FsbG93ZWQYCiABKAgSDwoHYWRkX251bRgLIAEoBRIKCgJwbhgMIAEo" + "CRJGCglvcGVyYXRvcnMYDSADKAsyMy5jb20ucHJvZ3JhbWlrYS5ycm8ud3Mu" + "Y2hrLlJyb0luZm9SZXNwb25zZS5PcGVyYXRvchIMCgR0aW5zGA4gASgJEgwK" + "BGxudW0YDyABKAUSEAoIbmFtZV9wYXkYECABKAkaSgoIT3BlcmF0b3ISDgoG" + "c2VyaWFsGAEgASgJEg4KBnN0YXR1cxgCIAEoBRIOCgZzZW5pb3IYAyABKAgS" + "DgoGaXNuYW1lGAQgASgJIr8BCgZTdGF0dXMSCwoHVU5LTk9XThAAEgYKAk9L" + "EAESGQoMRVJST1JfVkVSRUZZEP///////////wESGAoLRVJST1JfQ0hFQ0sQ" + "/v//////////ARIaCg1FUlJPUl9VTktOT1dOEPz//////////wESJQoYRVJS" + "T1JfTk9UX1JFR0lTVEVSRURfUlJPEPP//////////wESKAobRVJST1JfTk9U" + "X1JFR0lTVEVSRURfU0lHTkVSEPL//////////wEy+wUKEENoa0luY29tZVNl" + "cnZpY2USVQoHc2VuZENoaxIgLmNvbS5wcm9ncmFtaWthLnJyby53cy5jaGsu" + "Q2hlY2saKC5jb20ucHJvZ3JhbWlrYS5ycm8ud3MuY2hrLkNoZWNrUmVzcG9u" + "c2USVwoJc2VuZENoa1YyEiAuY29tLnByb2dyYW1pa2EucnJvLndzLmNoay5D" + "aGVjaxooLmNvbS5wcm9ncmFtaWthLnJyby53cy5jaGsuQ2hlY2tSZXNwb25z" + "ZRJcCgdsYXN0Q2hrEicuY29tLnByb2dyYW1pa2EucnJvLndzLmNoay5DaGVj" + "a1JlcXVlc3QaKC5jb20ucHJvZ3JhbWlrYS5ycm8ud3MuY2hrLkNoZWNrUmVz" + "cG9uc2USUgoEcGluZxIgLmNvbS5wcm9ncmFtaWthLnJyby53cy5jaGsuQ2hl" + "Y2saKC5jb20ucHJvZ3JhbWlrYS5ycm8ud3MuY2hrLkNoZWNrUmVzcG9uc2US" + "XwoKZGVsTGFzdENoaxInLmNvbS5wcm9ncmFtaWthLnJyby53cy5jaGsuQ2hl" + "Y2tSZXF1ZXN0GiguY29tLnByb2dyYW1pa2EucnJvLndzLmNoay5DaGVja1Jl" + "c3BvbnNlEmMKDGRlbExhc3RDaGtJZBIpLmNvbS5wcm9ncmFtaWthLnJyby53" + "cy5jaGsuQ2hlY2tSZXF1ZXN0SWQaKC5jb20ucHJvZ3JhbWlrYS5ycm8ud3Mu" + "Y2hrLkNoZWNrUmVzcG9uc2USXwoJc3RhdHVzUnJvEicuY29tLnByb2dyYW1p" + "a2EucnJvLndzLmNoay5DaGVja1JlcXVlc3QaKS5jb20ucHJvZ3JhbWlrYS5y" + "cm8ud3MuY2hrLlN0YXR1c1Jlc3BvbnNlEl4KB2luZm9Scm8SJy5jb20ucHJv" + "Z3JhbWlrYS5ycm8ud3MuY2hrLkNoZWNrUmVxdWVzdBoqLmNvbS5wcm9ncmFt" + "aWthLnJyby53cy5jaGsuUnJvSW5mb1Jlc3BvbnNlQgJQAWIGcHJvdG8z"), new FileDescriptor[0], new GeneratedClrTypeInfo(null, null, new GeneratedClrTypeInfo[6]
		{
			new GeneratedClrTypeInfo(typeof(Check), Check.Parser, new string[7] { "RroFn", "DateTime", "CheckSign", "LocalNumber", "CheckType", "IdOffline", "IdCancel" }, null, new Type[1] { typeof(Check.Types.Type) }, null, null),
			new GeneratedClrTypeInfo(typeof(CheckRequest), CheckRequest.Parser, new string[1] { "RroFnSign" }, null, null, null, null),
			new GeneratedClrTypeInfo(typeof(CheckRequestId), CheckRequestId.Parser, new string[2] { "Id", "RroFnSign" }, null, null, null, null),
			new GeneratedClrTypeInfo(typeof(CheckResponse), CheckResponse.Parser, new string[5] { "Id", "Status", "IdSign", "DataSign", "ErrorMessage" }, null, new Type[1] { typeof(CheckResponse.Types.Status) }, null, null),
			new GeneratedClrTypeInfo(typeof(StatusResponse), StatusResponse.Parser, new string[5] { "OpenShift", "Online", "LastSigner", "Status", "ErrorMessage" }, null, new Type[1] { typeof(StatusResponse.Types.Status) }, null, null),
			new GeneratedClrTypeInfo(typeof(RroInfoResponse), RroInfoResponse.Parser, new string[16]
			{
				"Status", "StatusRro", "OpenShift", "Online", "LastSigner", "Name", "NameTo", "Addr", "SingleTax", "OfflineAllowed",
				"AddNum", "Pn", "Operators", "Tins", "Lnum", "NamePay"
			}, null, new Type[1] { typeof(RroInfoResponse.Types.Status) }, null, new GeneratedClrTypeInfo[1]
			{
				new GeneratedClrTypeInfo(typeof(RroInfoResponse.Types.Operator), RroInfoResponse.Types.Operator.Parser, new string[4] { "Serial", "Status", "Senior", "Isname" }, null, null, null, null)
			})
		}));
	}
}
