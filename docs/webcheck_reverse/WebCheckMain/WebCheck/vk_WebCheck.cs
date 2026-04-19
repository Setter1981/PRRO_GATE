using System;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[ComVisible(true)]
[Guid("02E65142-0F38-4a10-9053-049F8F2B4024")]
[ProgId("AddIn.vk_WebCheck")]
public class vk_WebCheck : IInitDone, ILanguageExtender
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

	private const string c_AddinName = "vk_WebCheck";

	private ClassFiscal WebC;

	private string pFNvk;

	private string vkErrStr;

	private int vkErrInt;

	public vk_WebCheck()
	{
		WebC = new ClassFiscal();
		vkErrStr = "";
		vkErrInt = 0;
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
		bstrExtensionName = "vk_WebCheck";
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
		pExepInfo.bstrSource = "vk_WebCheck";
		V7Data.ErrorLog.AddError("vk_WebCheck", ref pExepInfo);
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
		try
		{
			pvarRetValue = 0;
			switch (lMethodNum)
			{
			case 1:
			{
				string text12 = Conversions.ToString(paParams.GetValue(0));
				text12 = "<Settings><Page Caption='Параметры'><Group Caption='Параметры подключения'><Parameter Name='FN' Caption='ФН' TypeValue='String'/><Parameter Name='KeyPath' Caption='Путь к ключу ЭЦП' TypeValue='String'/><Parameter Name='KeyPass' Caption='Пароль к ключу ЭЦП' TypeValue='String'/></Group></Page></Settings>";
				paParams.SetValue(text12, 0);
				if (All.d.VerifyXML(text12))
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
				string text29 = Conversions.ToString(paParams.GetValue(0));
				string text30 = Conversions.ToString(paParams.GetValue(1));
				text29 = text29.ToLower();
				switch (text29.Trim())
				{
				case "fn":
				case "фн":
					if (!All.A.Status)
					{
						pFNvk = text30;
						pvarRetValue = true;
					}
					else
					{
						pvarRetValue = false;
					}
					break;
				case "keypath":
					All.A.PathKey = text30;
					break;
				case "keypass":
					All.A.PassKey = text30;
					break;
				default:
					pvarRetValue = false;
					break;
				}
				break;
			}
			case 2:
			{
				string text28 = Conversions.ToString(paParams.GetValue(0));
				text28 = pFNvk;
				paParams.SetValue(text28, 0);
				string strFN3 = "<InputParameters><Parameters FN='" + pFNvk + "'/></InputParameters>";
				if (All.A.Status & (Operators.CompareString(pFNvk, All.A.FN, false) == 0))
				{
					text28 = All.A.FN;
					pvarRetValue = true;
				}
				else if (WebC.Initialization(strFN3))
				{
					text28 = All.A.FN;
					pvarRetValue = true;
				}
				else
				{
					vkErrStr = All.A.ErrHelp;
					vkErrInt = All.A.ErrCode;
					pvarRetValue = false;
				}
				break;
			}
			case 3:
			{
				string text25 = Conversions.ToString(paParams.GetValue(0));
				text25 = vkErrStr;
				paParams.SetValue(text25, 0);
				pvarRetValue = vkErrInt;
				break;
			}
			case 4:
			{
				string text24 = Conversions.ToString(paParams.GetValue(0));
				text24 = All.A.FN;
				paParams.SetValue(text24, 0);
				string strFN2 = "<InputParameters><Parameters FN='" + All.A.FN + "'/></InputParameters>";
				if (WebC.Finalization(strFN2))
				{
					pvarRetValue = true;
					break;
				}
				vkErrStr = All.A.ErrHelp;
				vkErrInt = All.A.ErrCode;
				pvarRetValue = false;
				break;
			}
			case 5:
			{
				string value2 = Conversions.ToString(paParams.GetValue(0));
				string text13 = Conversions.ToString(paParams.GetValue(1));
				string value3 = Conversions.ToString(paParams.GetValue(2));
				string value4 = Conversions.ToString(paParams.GetValue(3));
				string value5 = Conversions.ToString(paParams.GetValue(4));
				text13 = text13.ToLower();
				text13 = Strings.Replace(text13, "\"", "'", 1, -1, (CompareMethod)0);
				string text14 = "fn='" + All.A.FN + "' operatorid";
				text13 = Strings.Replace(text13, "cashiervatin", text14, 1, -1, (CompareMethod)0);
				if (WebC.OpenShift(text13))
				{
					value2 = All.A.FN;
					value3 = "<OutputParameters><Parameters UrgentReplacementFN='false' MemoryOverflowFN='false' ResourcesExhaustionFN='false' OFDtimeout='false' /></OutputParameters>";
					value4 = All.l.ReturnOpenShift().ReturnStr;
					value5 = All.NumberTaxVk;
					pvarRetValue = true;
				}
				else
				{
					vkErrStr = All.A.ErrHelp;
					vkErrInt = All.A.ErrCode;
					pvarRetValue = false;
				}
				paParams.SetValue(value2, 0);
				paParams.SetValue(text13, 1);
				paParams.SetValue(value3, 2);
				paParams.SetValue(value4, 3);
				paParams.SetValue(value5, 4);
				break;
			}
			case 6:
			{
				string value6 = Conversions.ToString(paParams.GetValue(0));
				string text19 = Conversions.ToString(paParams.GetValue(1));
				string value7 = Conversions.ToString(paParams.GetValue(2));
				string text20 = Conversions.ToString(paParams.GetValue(3));
				string value8 = Conversions.ToString(paParams.GetValue(4));
				text20 = All.l.ReturnOpenShift().ReturnStr;
				int num6 = All.Rf.NumberOfChecks(Conversions.ToInteger(text20));
				text19 = text19.ToLower();
				text19 = Strings.Replace(text19, "\"", "'", 1, -1, (CompareMethod)0);
				string text21 = "fn='" + All.A.FN + "' operatorid";
				text19 = Strings.Replace(text19, "cashiervatin", text21, 1, -1, (CompareMethod)0);
				if (WebC.ReportZ(text19))
				{
					value6 = All.A.FN;
					value7 = "<?xml version='1.0' encoding='UTF-8'?><OutputParameters><Parameters NumberOfChecks='" + num6 + "' NumberOfDocuments='0' BacklogDocumentsCounter='0' /></OutputParameters>";
					value8 = All.NumberTaxVk;
					pvarRetValue = true;
				}
				else
				{
					vkErrStr = All.A.ErrHelp;
					vkErrInt = All.A.ErrCode;
					pvarRetValue = false;
				}
				paParams.SetValue(value6, 0);
				paParams.SetValue(text19, 1);
				paParams.SetValue(value7, 2);
				paParams.SetValue(text20, 3);
				paParams.SetValue(value8, 4);
				break;
			}
			case 7:
			{
				string value9 = Conversions.ToString(paParams.GetValue(0));
				string text26 = Conversions.ToString(paParams.GetValue(1));
				double num7 = Conversions.ToDouble(paParams.GetValue(2));
				text26 = text26.ToLower();
				text26 = Strings.Replace(text26, "\"", "'", 1, -1, (CompareMethod)0);
				string text27 = "";
				if (num7 > 0.0)
				{
					text27 = "sumin='" + num7 + "' paymentid='1' fn='" + All.A.FN + "' operatorid";
				}
				else if (num7 < 0.0)
				{
					text27 = "sumout='" + Math.Abs(num7) + "' paymentid='1' fn='" + All.A.FN + "' operatorid";
				}
				text26 = Strings.Replace(text26, "cashiervatin", text27, 1, -1, (CompareMethod)0);
				if (WebC.CashInOut(text26))
				{
					pvarRetValue = true;
				}
				else
				{
					vkErrStr = All.A.ErrHelp;
					vkErrInt = All.A.ErrCode;
					pvarRetValue = false;
				}
				paParams.SetValue(value9, 0);
				paParams.SetValue(text26, 1);
				paParams.SetValue(num7, 2);
				break;
			}
			case 8:
			{
				string text4 = Conversions.ToString(paParams.GetValue(0));
				string text5 = Conversions.ToString(paParams.GetValue(1));
				string text6 = Conversions.ToString(paParams.GetValue(2));
				int num = Conversions.ToInteger(paParams.GetValue(3));
				string text7 = Conversions.ToString(Conversions.ToBoolean(paParams.GetValue(4)));
				string text8 = Conversions.ToString(Conversions.ToBoolean(paParams.GetValue(5)));
				string text9 = Conversions.ToString(paParams.GetValue(6));
				text4 = "ВебЧекПФР";
				text5 = "http://webchek.com.ua";
				text6 = "ККТ";
				num = 2005;
				text7 = Conversions.ToString(false);
				text8 = Conversions.ToString(false);
				text9 = "http://webchek.com.ua";
				paParams.SetValue(text4, 0);
				paParams.SetValue(text5, 1);
				paParams.SetValue(text6, 2);
				paParams.SetValue(num, 3);
				paParams.SetValue(text7, 4);
				paParams.SetValue(text8, 5);
				paParams.SetValue(text9, 6);
				pvarRetValue = true;
				break;
			}
			case 9:
			{
				string text33 = Conversions.ToString(paParams.GetValue(0));
				text33 = "<?xml version='1.0' encoding='UTF-8' ?>        \r\n        <Actions>\r\n        <Action Name='ShowWizardNewPro' Caption='Добавить фискальный номер'/> \r\n        <Action Name='ShowDataRRO' Caption='Посмотреть текущие настройки ФР'/>       \r\n        <Action Name='ShowOperators' Caption='Показать операторов'/> \r\n        <Action Name='ShowLicenseCheck' Caption='Показать доступные лицензии'/> \r\n        <Action Name ='ShowReports' Caption='Показать контрольную ленту ПРРО'/> \r\n        </Actions>";
				paParams.SetValue(text33, 0);
				pvarRetValue = true;
				break;
			}
			case 10:
				pvarRetValue = All.VersionDll();
				break;
			case 11:
			{
				string text10 = Conversions.ToString(paParams.GetValue(0));
				string text11 = Conversions.ToString(paParams.GetValue(1));
				text10 = "Ok";
				text11 = "free";
				if (All.A.Status)
				{
					text11 = (All.A.FullVersion ? All.A.Fullend : "free");
				}
				else
				{
					text11 = ((!All.InitializationNew("<InputParameters><Parameters FN='" + pFNvk + "'/></InputParameters>")) ? "free" : (All.A.FullVersion ? All.A.Fullend : "free"));
					All.FinalizationNew();
				}
				paParams.SetValue(text10, 0);
				paParams.SetValue(text11, 1);
				pvarRetValue = true;
				break;
			}
			case 12:
			{
				string text2 = Conversions.ToString(paParams.GetValue(0));
				string text3 = Conversions.ToString(paParams.GetValue(1));
				text2 = All.A.FN;
				text3 = "<?xml version='1.0' encoding='UTF-8'?><OutputParameters><Parameters KKTSerialNumber='0' Fiscal='True' FNSerialNumber='0' /></OutputParameters>";
				paParams.SetValue(text2, 0);
				paParams.SetValue(text3, 1);
				pvarRetValue = true;
				break;
			}
			case 13:
			{
				string text31 = Conversions.ToString(paParams.GetValue(0));
				int num8 = Conversions.ToInteger(paParams.GetValue(1));
				int num9 = Conversions.ToInteger(paParams.GetValue(2));
				int num10 = Conversions.ToInteger(paParams.GetValue(3));
				string text32 = Conversions.ToString(paParams.GetValue(4));
				text31 = All.A.FN;
				num9 = Conversions.ToInteger(All.l.ReturnOpenShift().ReturnStr);
				num8 = All.Rf.NumberOfChecks(num9);
				num10 = ((num9 <= 0) ? 1 : 2);
				text32 = "<?xml version='1.0' encoding='UTF-8'?><StatusParameters><Parameters BacklogDocumentsCounter='0' /></StatusParameters>";
				paParams.SetValue(text31, 0);
				paParams.SetValue(num8, 1);
				paParams.SetValue(num9, 2);
				paParams.SetValue(num10, 3);
				paParams.SetValue(text32, 4);
				pvarRetValue = true;
				break;
			}
			case 14:
			{
				bool flag = false;
				string text16 = Conversions.ToString(paParams.GetValue(0));
				flag = Conversions.ToBoolean(paParams.GetValue(1));
				string xmlVK = Conversions.ToString(paParams.GetValue(2));
				int num3 = Conversions.ToInteger(paParams.GetValue(3));
				int num4 = Conversions.ToInteger(paParams.GetValue(4));
				string text17 = Conversions.ToString(paParams.GetValue(5));
				string text18 = Conversions.ToString(paParams.GetValue(6));
				xmlVK = new Vk().XMLvkToCom(xmlVK);
				if (Operators.CompareString(All.d.GetParametrToString(xmlVK, "pc").ReturnStr.ToUpper(), "ВИДАЧА КОШТІВ", false) == 0)
				{
					if (WebC.EPZtoCash(xmlVK))
					{
						pvarRetValue = true;
					}
					else
					{
						vkErrStr = All.A.ErrHelp;
						vkErrInt = All.A.ErrCode;
						pvarRetValue = false;
					}
				}
				else if (WebC.FiscalReceipt(xmlVK))
				{
					pvarRetValue = true;
				}
				else
				{
					vkErrStr = All.A.ErrHelp;
					vkErrInt = All.A.ErrCode;
					pvarRetValue = false;
				}
				string returnStr = All.l.ReturnOpenShift().ReturnStr;
				int num5 = All.Rf.NumberOfChecks(Conversions.ToInteger(returnStr));
				text16 = All.A.FN;
				flag = false;
				num3 = num5;
				num4 = Conversions.ToInteger(returnStr);
				text17 = All.NumberTaxVk;
				text18 = "https://www.webchek.com.ua/";
				paParams.SetValue(text16, 0);
				paParams.SetValue(flag, 1);
				paParams.SetValue(xmlVK, 2);
				paParams.SetValue(num3, 3);
				paParams.SetValue(num4, 4);
				paParams.SetValue(text17, 5);
				paParams.SetValue(text18, 6);
				break;
			}
			case 15:
			{
				string text22 = Conversions.ToString(paParams.GetValue(0));
				text22 = text22.Trim();
				text22 = text22.ToLower();
				string strFN = "<InputParameters><Parameters FN='" + pFNvk + "'/></InputParameters>";
				pvarRetValue = true;
				string text23 = text22;
				if (Operators.CompareString(text23, "ShowWizardNewPro".ToLower(), false) == 0)
				{
					pvarRetValue = WebC.ShowWizardNewPro();
				}
				else if (Operators.CompareString(text23, "ShowDataRRO".ToLower(), false) == 0)
				{
					pvarRetValue = WebC.ShowDataRRO(strFN);
				}
				else if (Operators.CompareString(text23, "ShowOperators".ToLower(), false) == 0)
				{
					pvarRetValue = WebC.ShowOperators(strFN);
				}
				else if (Operators.CompareString(text23, "ShowLicenseCheck".ToLower(), false) == 0)
				{
					pvarRetValue = WebC.ShowLicenseCheck();
				}
				else if (Operators.CompareString(text23, "ShowReports".ToLower(), false) == 0)
				{
					pvarRetValue = WebC.ShowReports(strFN);
				}
				else
				{
					pvarRetValue = false;
					All.A.ErrHelp = "Ошибка компоненты. Дополнительные действия - такой команды нет.";
					All.A.ErrCode = 23;
				}
				if (Operators.ConditionalCompareObjectEqual(pvarRetValue, (object)false, false))
				{
					vkErrStr = All.A.ErrHelp;
					vkErrInt = All.A.ErrCode;
				}
				paParams.SetValue(text22, 0);
				break;
			}
			case 16:
			{
				string strXML2 = Conversions.ToString(paParams.GetValue(0));
				if (WebC.ShowPrintByCheckFn(strXML2))
				{
					pvarRetValue = true;
					break;
				}
				vkErrStr = All.A.ErrHelp;
				vkErrInt = All.A.ErrCode;
				pvarRetValue = false;
				break;
			}
			case 17:
			{
				string text15 = Conversions.ToString(paParams.GetValue(0));
				int num2 = Conversions.ToInteger(paParams.GetValue(1));
				text15 = pFNvk;
				num2 = 22;
				paParams.SetValue(text15, 0);
				paParams.SetValue(num2, 1);
				pvarRetValue = true;
				break;
			}
			case 18:
			{
				string text = Conversions.ToString(paParams.GetValue(0));
				string value = Conversions.ToString(paParams.GetValue(1));
				if (Operators.CompareString(text.Trim(), All.A.FN, false) == 0)
				{
					string strXML = "<InputParameters><Parameters fn='" + All.A.FN + "'/></InputParameters>";
					if (WebC.ReportX(strXML))
					{
						text = All.A.FN;
						pvarRetValue = true;
					}
					else
					{
						vkErrStr = All.A.ErrHelp;
						vkErrInt = All.A.ErrCode;
						pvarRetValue = false;
					}
				}
				else
				{
					vkErrStr = "Помилка компоненти. Під'єднано інший Фіскальний Номер.";
					vkErrInt = 2;
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
