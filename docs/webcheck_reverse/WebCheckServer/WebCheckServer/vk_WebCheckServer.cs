using System;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheckServer;

[ComVisible(true)]
[Guid("DE535CB8-E05F-4348-9178-EE95B3E55FF5")]
[ProgId("AddIn.vk_WebCheckServer")]
public class vk_WebCheckServer : IInitDone, ILanguageExtender
{
	public enum Props
	{
		pFn,
		LastProp
	}

	public enum Methods
	{
		SetP,
		GetP,
		Open,
		LastErr,
		Close,
		OpenShift,
		CloseShift,
		CashInOutcome,
		GetDescription,
		GetAdditionalActions,
		GetVersion,
		DeviceTest,
		GetDataKKT,
		GetCurrentStatus,
		ProcessCheck,
		DoAdditionalAction,
		PrintByCheckFn,
		GetLineLength,
		PrintXReport,
		LastMethod
	}

	private const string c_AddinName = "vk_WebCheckServer";

	private ClassFiscal WebC;

	private string pFNvk;

	private string vkErrStr;

	private int vkErrInt;

	private string vkXMLerr;

	private string vkXMLprt;

	public vk_WebCheckServer()
	{
		WebC = new ClassFiscal();
		vkErrStr = "";
		vkErrInt = 0;
		vkXMLerr = "";
		vkXMLprt = "";
	}

	private void Init([MarshalAs(UnmanagedType.IDispatch)] object pConnection)
	{
		V7Data.V7Object = RuntimeHelpers.GetObjectValue(pConnection);
	}

	void IInitDone.Init([MarshalAs(UnmanagedType.IDispatch)] object pConnection)
	{
		//ILSpy generated this explicit interface implementation from .override directive in Init
		this.Init(pConnection);
	}

	private void Done()
	{
		V7Data.V7Object = null;
		GC.Collect();
		GC.WaitForPendingFinalizers();
	}

	void IInitDone.Done()
	{
		//ILSpy generated this explicit interface implementation from .override directive in Done
		this.Done();
	}

	private void GetInfo(ref object[] pInfo)
	{
		pInfo.SetValue("0300", 0);
	}

	void IInitDone.GetInfo(ref object[] pInfo)
	{
		//ILSpy generated this explicit interface implementation from .override directive in GetInfo
		this.GetInfo(ref pInfo);
	}

	public void RegisterExtensionAs(ref string bstrExtensionName)
	{
		bstrExtensionName = "vk_WebCheckServer";
	}

	void ILanguageExtender.RegisterExtensionAs(ref string bstrExtensionName)
	{
		//ILSpy generated this explicit interface implementation from .override directive in RegisterExtensionAs
		this.RegisterExtensionAs(ref bstrExtensionName);
	}

	public void GetNProps(ref int plProps)
	{
		if (!All.A.Status)
		{
			plProps = 1;
		}
	}

	void ILanguageExtender.GetNProps(ref int plProps)
	{
		//ILSpy generated this explicit interface implementation from .override directive in GetNProps
		this.GetNProps(ref plProps);
	}

	public void FindProp(string bstrPropName, ref int plPropNum)
	{
		if (Operators.CompareString(bstrPropName, "vkFN", false) == 0 || Operators.CompareString(bstrPropName, "вкФН", false) == 0)
		{
			plPropNum = 0;
		}
		else
		{
			plPropNum = -1;
		}
	}

	void ILanguageExtender.FindProp(string bstrPropName, ref int plPropNum)
	{
		//ILSpy generated this explicit interface implementation from .override directive in FindProp
		this.FindProp(bstrPropName, ref plPropNum);
	}

	public void GetPropName(int lPropNum, int lAliasNum, ref string pbstrPropName)
	{
		if (lAliasNum == 1)
		{
			if (lPropNum == 0)
			{
				pbstrPropName = "вкФН";
			}
			else
			{
				pbstrPropName = "";
			}
		}
		else if (lPropNum == 0)
		{
			pbstrPropName = "vkFN";
		}
		else
		{
			pbstrPropName = "";
		}
	}

	void ILanguageExtender.GetPropName(int lPropNum, int lAliasNum, ref string pbstrPropName)
	{
		//ILSpy generated this explicit interface implementation from .override directive in GetPropName
		this.GetPropName(lPropNum, lAliasNum, ref pbstrPropName);
	}

	public void Raise1CException(string s)
	{
		ExcepInfo pExepInfo = default(ExcepInfo);
		pExepInfo.wCode = 1004;
		pExepInfo.scode = 1;
		pExepInfo.bstrDescription = s;
		pExepInfo.bstrSource = "vk_WebCheckServer";
		V7Data.ErrorLog.AddError("vk_WebCheckServer", ref pExepInfo);
	}

	public void GetPropVal(int lPropNum, ref object pvarPropVal)
	{
		try
		{
			pvarPropVal = null;
			if (lPropNum == 0)
			{
				pvarPropVal = pFNvk;
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			Raise1CException(ex2.Message);
			ProjectData.ClearProjectError();
		}
	}

	void ILanguageExtender.GetPropVal(int lPropNum, ref object pvarPropVal)
	{
		//ILSpy generated this explicit interface implementation from .override directive in GetPropVal
		this.GetPropVal(lPropNum, ref pvarPropVal);
	}

	public void SetPropVal(int lPropNum, ref object varPropVal)
	{
		if (lPropNum == 0)
		{
			pFNvk = Conversions.ToString(varPropVal);
		}
	}

	void ILanguageExtender.SetPropVal(int lPropNum, ref object varPropVal)
	{
		//ILSpy generated this explicit interface implementation from .override directive in SetPropVal
		this.SetPropVal(lPropNum, ref varPropVal);
	}

	public void IsPropReadable(int lPropNum, ref bool pboolPropRead)
	{
		pboolPropRead = true;
	}

	void ILanguageExtender.IsPropReadable(int lPropNum, ref bool pboolPropRead)
	{
		//ILSpy generated this explicit interface implementation from .override directive in IsPropReadable
		this.IsPropReadable(lPropNum, ref pboolPropRead);
	}

	public void IsPropWritable(int lPropNum, ref bool pboolPropWrite)
	{
		pboolPropWrite = true;
	}

	void ILanguageExtender.IsPropWritable(int lPropNum, ref bool pboolPropWrite)
	{
		//ILSpy generated this explicit interface implementation from .override directive in IsPropWritable
		this.IsPropWritable(lPropNum, ref pboolPropWrite);
	}

	public void GetNMethods(ref int plMethods)
	{
		plMethods = 19;
	}

	void ILanguageExtender.GetNMethods(ref int plMethods)
	{
		//ILSpy generated this explicit interface implementation from .override directive in GetNMethods
		this.GetNMethods(ref plMethods);
	}

	public void FindMethod(string bstrMethodName, ref int plMethodNum)
	{
		plMethodNum = -1;
		switch (bstrMethodName)
		{
		case "GetParameters":
		case "ПолучитьПараметры":
			plMethodNum = 1;
			break;
		case "SetParameter":
		case "УстановитьПараметр":
			plMethodNum = 0;
			break;
		case "Open":
		case "Подключить":
			plMethodNum = 2;
			break;
		case "GetLastError":
		case "ПолучитьОшибку":
			plMethodNum = 3;
			break;
		case "Close":
		case "Отключить":
			plMethodNum = 4;
			break;
		case "OpenShift":
		case "ОткрытьСмену":
			plMethodNum = 5;
			break;
		case "CloseShift":
		case "ЗакрытьСмену":
			plMethodNum = 6;
			break;
		case "CashInOutcome":
		case "НапечататьЧекВнесенияВыемки":
			plMethodNum = 7;
			break;
		case "GetDescription":
		case "ПолучитьОписание":
			plMethodNum = 8;
			break;
		case "GetAdditionalActions":
		case "ПолучитьДополнительныеДействия":
			plMethodNum = 9;
			break;
		case "GetVersion":
		case "ПолучитьНомерВерсии":
			plMethodNum = 10;
			break;
		case "DeviceTest":
		case "ТестУстройства":
			plMethodNum = 11;
			break;
		case "GetDataKKT":
		case "ПолучитьПараметрыККТ":
			plMethodNum = 12;
			break;
		case "GetCurrentStatus":
		case "ПолучитьТекущееСостояние":
			plMethodNum = 13;
			break;
		case "ProcessCheck":
		case "СформироватьЧек":
			plMethodNum = 14;
			break;
		case "DoAdditionalAction":
		case "ВыполнитьДополнительноеДействие":
			plMethodNum = 15;
			break;
		case "PrintByCheckFn":
		case "ПечатьЧека":
			plMethodNum = 16;
			break;
		case "GetLineLength":
		case "ПолучитьШиринуСтроки":
			plMethodNum = 17;
			break;
		case "PrintXReport":
		case "НапечататьОтчетБезГашения":
			plMethodNum = 18;
			break;
		}
	}

	void ILanguageExtender.FindMethod(string bstrMethodName, ref int plMethodNum)
	{
		//ILSpy generated this explicit interface implementation from .override directive in FindMethod
		this.FindMethod(bstrMethodName, ref plMethodNum);
	}

	public void GetMethodName(int lMethodNum, int lAliasNum, ref string pbstrMethName)
	{
		if (lAliasNum == 1)
		{
			switch (lMethodNum)
			{
			case 1:
				pbstrMethName = "ПолучитьПараметры";
				break;
			case 0:
				pbstrMethName = "УстановитьПараметр";
				break;
			case 2:
				pbstrMethName = "Подключить";
				break;
			case 3:
				pbstrMethName = "ПолучитьОшибку";
				break;
			case 4:
				pbstrMethName = "Отключить";
				break;
			case 5:
				pbstrMethName = "ОткрытьСмену";
				break;
			case 6:
				pbstrMethName = "ЗакрытьСмену";
				break;
			case 7:
				pbstrMethName = "НапечататьЧекВнесенияВыемки";
				break;
			case 8:
				pbstrMethName = "ПолучитьОписание";
				break;
			case 9:
				pbstrMethName = "ПолучитьДополнительныеДействия";
				break;
			case 10:
				pbstrMethName = "ПолучитьНомерВерсии";
				break;
			case 11:
				pbstrMethName = "ТестУстройства";
				break;
			case 12:
				pbstrMethName = "ПолучитьПараметрыККТ";
				break;
			case 13:
				pbstrMethName = "ПолучитьТекущееСостояние";
				break;
			case 14:
				pbstrMethName = "СформироватьЧек";
				break;
			case 15:
				pbstrMethName = "ВыполнитьДополнительноеДействие";
				break;
			case 16:
				pbstrMethName = "ПечатьЧека";
				break;
			case 17:
				pbstrMethName = "ПолучитьШиринуСтроки";
				break;
			case 18:
				pbstrMethName = "НапечататьОтчетБезГашения";
				break;
			default:
				pbstrMethName = "";
				break;
			}
		}
		else
		{
			switch (lMethodNum)
			{
			case 1:
				pbstrMethName = "GetParameters";
				break;
			case 0:
				pbstrMethName = "SetParameter";
				break;
			case 2:
				pbstrMethName = "Open";
				break;
			case 3:
				pbstrMethName = "GetLastError";
				break;
			case 4:
				pbstrMethName = "Close";
				break;
			case 5:
				pbstrMethName = "OpenShift";
				break;
			case 6:
				pbstrMethName = "CloseShift";
				break;
			case 7:
				pbstrMethName = "CashInOutcome";
				break;
			case 8:
				pbstrMethName = "GetDescription";
				break;
			case 9:
				pbstrMethName = "GetAdditionalActions";
				break;
			case 10:
				pbstrMethName = "GetVersion";
				break;
			case 11:
				pbstrMethName = "DeviceTest";
				break;
			case 12:
				pbstrMethName = "GetDataKKT";
				break;
			case 13:
				pbstrMethName = "GetCurrentStatus";
				break;
			case 14:
				pbstrMethName = "ProcessCheck";
				break;
			case 15:
				pbstrMethName = "DoAdditionalAction";
				break;
			case 16:
				pbstrMethName = "PrintByCheckFn";
				break;
			case 17:
				pbstrMethName = "GetLineLength";
				break;
			case 18:
				pbstrMethName = "PrintXReport";
				break;
			default:
				pbstrMethName = "";
				break;
			}
		}
	}

	void ILanguageExtender.GetMethodName(int lMethodNum, int lAliasNum, ref string pbstrMethName)
	{
		//ILSpy generated this explicit interface implementation from .override directive in GetMethodName
		this.GetMethodName(lMethodNum, lAliasNum, ref pbstrMethName);
	}

	public void GetNParams(int lMethodNum, ref int plParams)
	{
		switch (lMethodNum)
		{
		case 1:
			plParams = 1;
			break;
		case 0:
			plParams = 2;
			break;
		case 2:
			plParams = 1;
			break;
		case 3:
			plParams = 1;
			break;
		case 4:
			plParams = 1;
			break;
		case 5:
			plParams = 5;
			break;
		case 6:
			plParams = 5;
			break;
		case 7:
			plParams = 3;
			break;
		case 8:
			plParams = 7;
			break;
		case 9:
			plParams = 1;
			break;
		case 10:
			plParams = 0;
			break;
		case 11:
			plParams = 2;
			break;
		case 12:
			plParams = 2;
			break;
		case 13:
			plParams = 5;
			break;
		case 14:
			plParams = 7;
			break;
		case 15:
			plParams = 1;
			break;
		case 16:
			plParams = 1;
			break;
		case 17:
			plParams = 2;
			break;
		case 18:
			plParams = 2;
			break;
		}
	}

	void ILanguageExtender.GetNParams(int lMethodNum, ref int plParams)
	{
		//ILSpy generated this explicit interface implementation from .override directive in GetNParams
		this.GetNParams(lMethodNum, ref plParams);
	}

	public void GetParamDefValue(int lMethodNum, int lParamNum, ref object pvarParamDefValue)
	{
		pvarParamDefValue = null;
	}

	void ILanguageExtender.GetParamDefValue(int lMethodNum, int lParamNum, ref object pvarParamDefValue)
	{
		//ILSpy generated this explicit interface implementation from .override directive in GetParamDefValue
		this.GetParamDefValue(lMethodNum, lParamNum, ref pvarParamDefValue);
	}

	public void HasRetVal(int lMethodNum, ref bool pboolRetValue)
	{
		pboolRetValue = true;
	}

	void ILanguageExtender.HasRetVal(int lMethodNum, ref bool pboolRetValue)
	{
		//ILSpy generated this explicit interface implementation from .override directive in HasRetVal
		this.HasRetVal(lMethodNum, ref pboolRetValue);
	}

	public void CallAsProc(int lMethodNum, ref Array paParams)
	{
	}

	void ILanguageExtender.CallAsProc(int lMethodNum, ref Array paParams)
	{
		//ILSpy generated this explicit interface implementation from .override directive in CallAsProc
		this.CallAsProc(lMethodNum, ref paParams);
	}

	public void CallAsFunc(int lMethodNum, ref object pvarRetValue, ref Array paParams)
	{
		if (lMethodNum != 3)
		{
			vkErrInt = 0;
			vkErrStr = "";
		}
		if (lMethodNum != 12)
		{
			vkXMLerr = "";
			vkXMLprt = "";
		}
		try
		{
			pvarRetValue = 0;
			switch (lMethodNum)
			{
			case 1:
			{
				string text30 = Conversions.ToString(paParams.GetValue(0));
				text30 = "<Settings><Page Caption='Параметры'><Group Caption='Параметры подключения'><Parameter Name='FN' Caption='ФН' TypeValue='String'/></Group></Page></Settings>";
				paParams.SetValue(text30, 0);
				if (All.VerifyXML(text30))
				{
					pvarRetValue = true;
				}
				else
				{
					pvarRetValue = false;
				}
				break;
			}
			case 0:
			{
				string text25 = Conversions.ToString(paParams.GetValue(0));
				string text26 = Conversions.ToString(paParams.GetValue(1));
				text25 = text25.ToLower();
				switch (text25.Trim())
				{
				case "fn":
				case "фн":
					if (!All.A.Status)
					{
						pFNvk = text26;
						pvarRetValue = true;
					}
					else
					{
						pvarRetValue = false;
					}
					break;
				case "keypath":
					All.A.PathKey = text26;
					break;
				case "keypass":
					All.A.PassKey = text26;
					break;
				default:
					pvarRetValue = false;
					break;
				}
				break;
			}
			case 2:
			{
				string text3 = Conversions.ToString(paParams.GetValue(0));
				text3 = pFNvk;
				paParams.SetValue(text3, 0);
				string strFN2 = "<InputParameters><Parameters FN='" + pFNvk + "'/></InputParameters>";
				if (WebC.Initialization(strFN2))
				{
					TypReply typReply3 = All.ReplyRemember(pFNvk);
					vkXMLerr = typReply3.ReplyErr;
					vkXMLprt = typReply3.ReplyPrt;
					text3 = pFNvk;
					pvarRetValue = true;
				}
				else
				{
					TypReply typReply4 = All.ReplyRemember(pFNvk);
					vkXMLerr = typReply4.ReplyErr;
					vkXMLprt = typReply4.ReplyPrt;
					vkErrStr = typReply4.ReplyErr;
					vkErrInt = 9999;
					pvarRetValue = false;
				}
				break;
			}
			case 3:
			{
				string text2 = Conversions.ToString(paParams.GetValue(0));
				text2 = vkErrStr;
				paParams.SetValue(text2, 0);
				pvarRetValue = vkErrInt;
				break;
			}
			case 4:
			{
				string text7 = Conversions.ToString(paParams.GetValue(0));
				text7 = (pFNvk = pFNvk);
				paParams.SetValue(text7, 0);
				string strFN3 = "<InputParameters><Parameters FN='" + pFNvk + "'/></InputParameters>";
				if (WebC.Finalization(strFN3))
				{
					TypReply typReply7 = All.ReplyRemember(pFNvk);
					vkXMLerr = typReply7.ReplyErr;
					vkXMLprt = typReply7.ReplyPrt;
					pvarRetValue = true;
				}
				else
				{
					TypReply typReply8 = All.ReplyRemember(pFNvk);
					vkXMLerr = typReply8.ReplyErr;
					vkXMLprt = typReply8.ReplyPrt;
					vkErrStr = typReply8.ReplyErr;
					vkErrInt = 9999;
					pvarRetValue = false;
				}
				break;
			}
			case 5:
			{
				string value2 = Conversions.ToString(paParams.GetValue(0));
				string text4 = Conversions.ToString(paParams.GetValue(1));
				string value3 = Conversions.ToString(paParams.GetValue(2));
				string value4 = Conversions.ToString(paParams.GetValue(3));
				string value5 = Conversions.ToString(paParams.GetValue(4));
				pFNvk = value2;
				text4 = text4.ToLower();
				text4 = Strings.Replace(text4, "\"", "'", 1, -1, (CompareMethod)0);
				string text5 = "fn='" + pFNvk + "' operatorid";
				text4 = Strings.Replace(text4, "cashiervatin", text5, 1, -1, (CompareMethod)0);
				if (WebC.OpenShift(text4))
				{
					TypReply typReply5 = All.ReplyRemember(pFNvk);
					vkXMLerr = typReply5.ReplyErr;
					vkXMLprt = typReply5.ReplyPrt;
					value2 = pFNvk;
					value3 = "<OutputParameters><Parameters UrgentReplacementFN='false' MemoryOverflowFN='false' ResourcesExhaustionFN='false' OFDtimeout='false' /></OutputParameters>";
					value4 = "8";
					value5 = All.NumberTaxVk;
					pvarRetValue = true;
				}
				else
				{
					TypReply typReply6 = All.ReplyRemember(pFNvk);
					vkXMLerr = typReply6.ReplyErr;
					vkXMLprt = typReply6.ReplyPrt;
					vkErrStr = typReply6.ReplyErr;
					vkErrInt = 9999;
					pvarRetValue = false;
				}
				paParams.SetValue(value2, 0);
				paParams.SetValue(text4, 1);
				paParams.SetValue(value3, 2);
				paParams.SetValue(value4, 3);
				paParams.SetValue(value5, 4);
				break;
			}
			case 6:
			{
				string value10 = Conversions.ToString(paParams.GetValue(0));
				string text22 = Conversions.ToString(paParams.GetValue(1));
				string value11 = Conversions.ToString(paParams.GetValue(2));
				string text23 = Conversions.ToString(paParams.GetValue(3));
				string value12 = Conversions.ToString(paParams.GetValue(4));
				pFNvk = value10;
				text23 = "8";
				int num7 = Conversions.ToInteger("7");
				text22 = text22.ToLower();
				text22 = Strings.Replace(text22, "\"", "'", 1, -1, (CompareMethod)0);
				string text24 = "fn='" + pFNvk + "' operatorid";
				text22 = Strings.Replace(text22, "cashiervatin", text24, 1, -1, (CompareMethod)0);
				if (WebC.ReportZ(text22))
				{
					TypReply typReply11 = All.ReplyRemember(pFNvk);
					vkXMLerr = typReply11.ReplyErr;
					vkXMLprt = typReply11.ReplyPrt;
					value10 = pFNvk;
					value11 = "<?xml version='1.0' encoding='UTF-8'?><OutputParameters><Parameters NumberOfChecks='" + num7 + "' NumberOfDocuments='0' BacklogDocumentsCounter='0' /></OutputParameters>";
					value12 = All.NumberTaxVk;
					pvarRetValue = true;
				}
				else
				{
					TypReply typReply12 = All.ReplyRemember(pFNvk);
					vkXMLerr = typReply12.ReplyErr;
					vkXMLprt = typReply12.ReplyPrt;
					vkErrStr = typReply12.ReplyErr;
					vkErrInt = 9999;
					pvarRetValue = false;
				}
				paParams.SetValue(value10, 0);
				paParams.SetValue(text22, 1);
				paParams.SetValue(value11, 2);
				paParams.SetValue(text23, 3);
				paParams.SetValue(value12, 4);
				break;
			}
			case 7:
			{
				string value9 = Conversions.ToString(paParams.GetValue(0));
				string text20 = Conversions.ToString(paParams.GetValue(1));
				double num6 = Conversions.ToDouble(paParams.GetValue(2));
				pFNvk = value9;
				text20 = text20.ToLower();
				text20 = Strings.Replace(text20, "\"", "'", 1, -1, (CompareMethod)0);
				string text21 = "";
				if (num6 > 0.0)
				{
					text21 = "sumin='" + num6 + "' paymentid='1' fn='" + pFNvk + "' operatorid";
				}
				else if (num6 < 0.0)
				{
					text21 = "sumout='" + Math.Abs(num6) + "' paymentid='1' fn='" + pFNvk + "' operatorid";
				}
				text20 = Strings.Replace(text20, "cashiervatin", text21, 1, -1, (CompareMethod)0);
				if (WebC.CashInOut(text20))
				{
					TypReply typReply9 = All.ReplyRemember(pFNvk);
					vkXMLerr = typReply9.ReplyErr;
					vkXMLprt = typReply9.ReplyPrt;
					pvarRetValue = true;
				}
				else
				{
					TypReply typReply10 = All.ReplyRemember(pFNvk);
					vkXMLerr = typReply10.ReplyErr;
					vkXMLprt = typReply10.ReplyPrt;
					vkErrStr = typReply10.ReplyErr;
					vkErrInt = 9999;
					pvarRetValue = false;
				}
				paParams.SetValue(value9, 0);
				paParams.SetValue(text20, 1);
				paParams.SetValue(num6, 2);
				break;
			}
			case 8:
			{
				string text14 = Conversions.ToString(paParams.GetValue(0));
				string text15 = Conversions.ToString(paParams.GetValue(1));
				string text16 = Conversions.ToString(paParams.GetValue(2));
				int num5 = Conversions.ToInteger(paParams.GetValue(3));
				string text17 = Conversions.ToString(Conversions.ToBoolean(paParams.GetValue(4)));
				string text18 = Conversions.ToString(Conversions.ToBoolean(paParams.GetValue(5)));
				string text19 = Conversions.ToString(paParams.GetValue(6));
				text14 = "ВебЧекПФР";
				text15 = "http://webchek.com.ua";
				text16 = "ККТ";
				num5 = 2005;
				text17 = Conversions.ToString(false);
				text18 = Conversions.ToString(false);
				text19 = "http://webchek.com.ua";
				paParams.SetValue(text14, 0);
				paParams.SetValue(text15, 1);
				paParams.SetValue(text16, 2);
				paParams.SetValue(num5, 3);
				paParams.SetValue(text17, 4);
				paParams.SetValue(text18, 5);
				paParams.SetValue(text19, 6);
				pvarRetValue = true;
				break;
			}
			case 9:
			{
				string text11 = Conversions.ToString(paParams.GetValue(0));
				text11 = "<?xml version='1.0' encoding='UTF-8' ?>        \r\n        <Actions>\r\n        <Action Name='ShowWizardNewPro' Caption='Добавить фискальный номер'/> \r\n        <Action Name='ShowDataRRO' Caption='Посмотреть текущие настройки ФР'/>       \r\n        <Action Name='ShowOperators' Caption='Показать операторов'/> \r\n        <Action Name='ShowLicenseCheck' Caption='Показать доступные лицензии'/> \r\n        <Action Name ='ShowReports' Caption='Показать контрольную ленту ПРРО'/> \r\n        </Actions>";
				paParams.SetValue(text11, 0);
				pvarRetValue = true;
				break;
			}
			case 10:
				pvarRetValue = "1.3.5";
				break;
			case 11:
			{
				string text12 = Conversions.ToString(paParams.GetValue(0));
				string text13 = Conversions.ToString(paParams.GetValue(1));
				text12 = "Ok";
				text13 = "free";
				if (All.A.Status)
				{
					text13 = (All.A.FullVersion ? All.A.Fullend : "free");
				}
				else
				{
					string strFN4 = "<InputParameters><Parameters FN='" + pFNvk + "'/></InputParameters>";
					text13 = ((!WebC.Initialization(strFN4)) ? "free" : (All.A.FullVersion ? All.A.Fullend : "free"));
					WebC.Finalization(strFN4);
				}
				paParams.SetValue(text12, 0);
				paParams.SetValue(text13, 1);
				pvarRetValue = true;
				break;
			}
			case 12:
			{
				string value8 = Conversions.ToString(paParams.GetValue(0));
				string text10 = Conversions.ToString(paParams.GetValue(1));
				pFNvk = value8;
				text10 = vkXMLprt;
				if (Operators.CompareString(text10.Trim(), "", false) == 0)
				{
					text10 = vkXMLerr;
				}
				paParams.SetValue(value8, 0);
				paParams.SetValue(text10, 1);
				pvarRetValue = true;
				break;
			}
			case 13:
			{
				string value6 = Conversions.ToString(paParams.GetValue(0));
				int num = Conversions.ToInteger(paParams.GetValue(1));
				int num2 = Conversions.ToInteger(paParams.GetValue(2));
				int num3 = Conversions.ToInteger(paParams.GetValue(3));
				string text6 = Conversions.ToString(paParams.GetValue(4));
				pFNvk = value6;
				num2 = 8;
				num = 88;
				num3 = ((num2 <= 0) ? 1 : 2);
				text6 = "<?xml version='1.0' encoding='UTF-8'?><StatusParameters><Parameters BacklogDocumentsCounter='0' /></StatusParameters>";
				paParams.SetValue(value6, 0);
				paParams.SetValue(num, 1);
				paParams.SetValue(num2, 2);
				paParams.SetValue(num3, 3);
				paParams.SetValue(text6, 4);
				pvarRetValue = true;
				break;
			}
			case 14:
			{
				bool flag = false;
				string fN = Conversions.ToString(paParams.GetValue(0));
				flag = Conversions.ToBoolean(paParams.GetValue(1));
				string text27 = Conversions.ToString(paParams.GetValue(2));
				int num8 = Conversions.ToInteger(paParams.GetValue(3));
				int num9 = Conversions.ToInteger(paParams.GetValue(4));
				string text28 = Conversions.ToString(paParams.GetValue(5));
				string text29 = Conversions.ToString(paParams.GetValue(6));
				pFNvk = fN;
				All.A.FN = fN;
				if (WebC.FiscalReceipt(text27))
				{
					TypReply typReply13 = All.ReplyRemember(pFNvk);
					vkXMLerr = typReply13.ReplyErr;
					vkXMLprt = typReply13.ReplyPrt;
					pvarRetValue = true;
				}
				else
				{
					TypReply typReply14 = All.ReplyRemember(pFNvk);
					vkXMLerr = typReply14.ReplyErr;
					vkXMLprt = typReply14.ReplyPrt;
					vkErrStr = typReply14.ReplyErr;
					vkErrInt = 9999;
					pvarRetValue = false;
				}
				fN = pFNvk;
				flag = false;
				num8 = 88;
				num9 = Conversions.ToInteger("8");
				text28 = All.NumberTaxVk;
				text29 = "https://www.webchek.com.ua/";
				paParams.SetValue(fN, 0);
				paParams.SetValue(flag, 1);
				paParams.SetValue(text27, 2);
				paParams.SetValue(num8, 3);
				paParams.SetValue(num9, 4);
				paParams.SetValue(text28, 5);
				paParams.SetValue(text29, 6);
				break;
			}
			case 15:
			{
				string text8 = Conversions.ToString(paParams.GetValue(0));
				text8 = text8.Trim();
				text8 = text8.ToLower();
				_ = "<InputParameters><Parameters FN='" + pFNvk + "'/></InputParameters>";
				pvarRetValue = true;
				string text9 = text8;
				if (Operators.CompareString(text9, "ShowWizardNewPro".ToLower(), false) != 0 && Operators.CompareString(text9, "ShowDataRRO".ToLower(), false) != 0 && Operators.CompareString(text9, "ShowOperators".ToLower(), false) != 0 && Operators.CompareString(text9, "ShowLicenseCheck".ToLower(), false) != 0 && Operators.CompareString(text9, "ShowReports".ToLower(), false) != 0)
				{
					pvarRetValue = false;
					All.A.ErrHelp = "Ошибка компоненты. Дополнительные действия - такой команды нет.";
					All.A.ErrCode = 81;
				}
				if (Operators.ConditionalCompareObjectEqual(pvarRetValue, (object)false, false))
				{
					vkErrStr = All.A.ErrHelp;
					vkErrInt = All.A.ErrCode;
				}
				paParams.SetValue(text8, 0);
				break;
			}
			case 16:
				Conversions.ToString(paParams.GetValue(0));
				break;
			case 17:
			{
				string value7 = Conversions.ToString(paParams.GetValue(0));
				int num4 = Conversions.ToInteger(paParams.GetValue(1));
				pFNvk = value7;
				num4 = 22;
				paParams.SetValue(value7, 0);
				paParams.SetValue(num4, 1);
				pvarRetValue = true;
				break;
			}
			case 18:
			{
				string text = Conversions.ToString(paParams.GetValue(0));
				string value = Conversions.ToString(paParams.GetValue(1));
				if (Operators.CompareString(text.Trim(), pFNvk, false) == 0)
				{
					string strFN = "<InputParameters><Parameters fn='" + All.A.FN + "'/></InputParameters>";
					if (WebC.ReportX(strFN))
					{
						TypReply typReply = All.ReplyRemember(pFNvk);
						vkXMLerr = typReply.ReplyErr;
						vkXMLprt = typReply.ReplyPrt;
						text = pFNvk;
						pvarRetValue = true;
					}
					else
					{
						TypReply typReply2 = All.ReplyRemember(pFNvk);
						vkXMLerr = typReply2.ReplyErr;
						vkXMLprt = typReply2.ReplyPrt;
						vkErrStr = typReply2.ReplyErr;
						vkErrInt = 9999;
						pvarRetValue = false;
					}
				}
				else
				{
					pvarRetValue = false;
				}
				paParams.SetValue(text, 0);
				paParams.SetValue(value, 1);
				break;
			}
			default:
				pvarRetValue = false;
				break;
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			Raise1CException(ex2.Message);
			ProjectData.ClearProjectError();
		}
	}

	void ILanguageExtender.CallAsFunc(int lMethodNum, ref object pvarRetValue, ref Array paParams)
	{
		//ILSpy generated this explicit interface implementation from .override directive in CallAsFunc
		this.CallAsFunc(lMethodNum, ref pvarRetValue, ref paParams);
	}
}
