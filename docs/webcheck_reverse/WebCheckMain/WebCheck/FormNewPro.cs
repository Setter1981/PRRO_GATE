using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Net;
using System.Runtime.CompilerServices;
using System.Text;
using System.Windows.Forms;
using Amazon;
using Amazon.Runtime.CredentialManagement;
using Amazon.S3;
using Amazon.S3.Model;
using Ionic.Zip;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.Devices;
using Microsoft.VisualBasic.FileIO;

namespace WebCheck;

[DesignerGenerated]
internal class FormNewPro : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("FnT")]
	private TextBox _FnT;

	[CompilerGenerated]
	[AccessedThroughProperty("KeyB")]
	private Button _KeyB;

	[CompilerGenerated]
	[AccessedThroughProperty("OkB")]
	private Button _OkB;

	[CompilerGenerated]
	[AccessedThroughProperty("CheckBoxTest")]
	private CheckBox _CheckBoxTest;

	[CompilerGenerated]
	[AccessedThroughProperty("SelSwrver")]
	private Button _SelSwrver;

	[CompilerGenerated]
	[AccessedThroughProperty("NoB")]
	private Button _NoB;

	[CompilerGenerated]
	[AccessedThroughProperty("Adr")]
	private Button _Adr;

	[CompilerGenerated]
	[AccessedThroughProperty("NamT")]
	private Button _NamT;

	[CompilerGenerated]
	[AccessedThroughProperty("NamO")]
	private Button _NamO;

	[CompilerGenerated]
	[AccessedThroughProperty("Pas")]
	private Button _Pas;

	[CompilerGenerated]
	[AccessedThroughProperty("INN")]
	private Button _INN;

	[CompilerGenerated]
	[AccessedThroughProperty("FIO")]
	private Button _FIO;

	[CompilerGenerated]
	[AccessedThroughProperty("InfaTaxPay")]
	private Button _InfaTaxPay;

	[CompilerGenerated]
	[AccessedThroughProperty("IPN")]
	private Button _IPN;

	[CompilerGenerated]
	[AccessedThroughProperty("EDP")]
	private Button _EDP;

	[CompilerGenerated]
	[AccessedThroughProperty("TestPro")]
	private Button _TestPro;

	[CompilerGenerated]
	[AccessedThroughProperty("ImportDat")]
	private Button _ImportDat;

	[CompilerGenerated]
	[AccessedThroughProperty("FNN")]
	private Button _FNN;

	[CompilerGenerated]
	[AccessedThroughProperty("CheckBoxManual")]
	private CheckBox _CheckBoxManual;

	private bool NewBase;

	private string ParOld;

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("GroupBox1")]
	internal virtual GroupBox GroupBox1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("GroupBox2")]
	internal virtual GroupBox GroupBox2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TinT")]
	internal virtual TextBox TinT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label2")]
	internal virtual Label Label2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label3")]
	internal virtual Label Label3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual TextBox FnT
	{
		[CompilerGenerated]
		get
		{
			return _FnT;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = FnT_TextChanged;
			TextBox fnT = _FnT;
			if (fnT != null)
			{
				((Control)fnT).TextChanged -= eventHandler;
			}
			_FnT = value;
			fnT = _FnT;
			if (fnT != null)
			{
				((Control)fnT).TextChanged += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("NtorgT")]
	internal virtual TextBox NtorgT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("NorgT")]
	internal virtual TextBox NorgT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("InnT")]
	internal virtual TextBox InnT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label7")]
	internal virtual Label Label7
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("AtorgT")]
	internal virtual TextBox AtorgT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label6")]
	internal virtual Label Label6
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label5")]
	internal virtual Label Label5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label4")]
	internal virtual Label Label4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label8")]
	internal virtual Label Label8
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button KeyB
	{
		[CompilerGenerated]
		get
		{
			return _KeyB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = KeyB_Click;
			Button keyB = _KeyB;
			if (keyB != null)
			{
				((Control)keyB).Click -= eventHandler;
			}
			_KeyB = value;
			keyB = _KeyB;
			if (keyB != null)
			{
				((Control)keyB).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Label10")]
	internal virtual Label Label10
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label9")]
	internal virtual Label Label9
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("PasOpT")]
	internal virtual TextBox PasOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("KeyOpT")]
	internal virtual TextBox KeyOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("InnOpT")]
	internal virtual TextBox InnOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("FioOpT")]
	internal virtual TextBox FioOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label11")]
	internal virtual Label Label11
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button OkB
	{
		[CompilerGenerated]
		get
		{
			return _OkB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = OkB_Click;
			Button okB = _OkB;
			if (okB != null)
			{
				((Control)okB).Click -= eventHandler;
			}
			_OkB = value;
			okB = _OkB;
			if (okB != null)
			{
				((Control)okB).Click += eventHandler;
			}
		}
	}

	internal virtual CheckBox CheckBoxTest
	{
		[CompilerGenerated]
		get
		{
			return _CheckBoxTest;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = CheckBoxTest_CheckedChanged;
			CheckBox checkBoxTest = _CheckBoxTest;
			if (checkBoxTest != null)
			{
				checkBoxTest.CheckedChanged -= eventHandler;
			}
			_CheckBoxTest = value;
			checkBoxTest = _CheckBoxTest;
			if (checkBoxTest != null)
			{
				checkBoxTest.CheckedChanged += eventHandler;
			}
		}
	}

	internal virtual Button SelSwrver
	{
		[CompilerGenerated]
		get
		{
			return _SelSwrver;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = SelSwrver_Click;
			Button selSwrver = _SelSwrver;
			if (selSwrver != null)
			{
				((Control)selSwrver).Click -= eventHandler;
			}
			_SelSwrver = value;
			selSwrver = _SelSwrver;
			if (selSwrver != null)
			{
				((Control)selSwrver).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("GroupBox3")]
	internal virtual GroupBox GroupBox3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Server")]
	internal virtual TextBox Server
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label21")]
	internal virtual Label Label21
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button NoB
	{
		[CompilerGenerated]
		get
		{
			return _NoB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = NoB_Click;
			Button noB = _NoB;
			if (noB != null)
			{
				((Control)noB).Click -= eventHandler;
			}
			_NoB = value;
			noB = _NoB;
			if (noB != null)
			{
				((Control)noB).Click += eventHandler;
			}
		}
	}

	internal virtual Button Adr
	{
		[CompilerGenerated]
		get
		{
			return _Adr;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Adr_Click;
			Button adr = _Adr;
			if (adr != null)
			{
				((Control)adr).Click -= eventHandler;
			}
			_Adr = value;
			adr = _Adr;
			if (adr != null)
			{
				((Control)adr).Click += eventHandler;
			}
		}
	}

	internal virtual Button NamT
	{
		[CompilerGenerated]
		get
		{
			return _NamT;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = NamT_Click;
			Button namT = _NamT;
			if (namT != null)
			{
				((Control)namT).Click -= eventHandler;
			}
			_NamT = value;
			namT = _NamT;
			if (namT != null)
			{
				((Control)namT).Click += eventHandler;
			}
		}
	}

	internal virtual Button NamO
	{
		[CompilerGenerated]
		get
		{
			return _NamO;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = NamO_Click;
			Button namO = _NamO;
			if (namO != null)
			{
				((Control)namO).Click -= eventHandler;
			}
			_NamO = value;
			namO = _NamO;
			if (namO != null)
			{
				((Control)namO).Click += eventHandler;
			}
		}
	}

	internal virtual Button Pas
	{
		[CompilerGenerated]
		get
		{
			return _Pas;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Pas_Click;
			Button pas = _Pas;
			if (pas != null)
			{
				((Control)pas).Click -= eventHandler;
			}
			_Pas = value;
			pas = _Pas;
			if (pas != null)
			{
				((Control)pas).Click += eventHandler;
			}
		}
	}

	internal virtual Button INN
	{
		[CompilerGenerated]
		get
		{
			return _INN;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = INN_Click;
			Button iNN = _INN;
			if (iNN != null)
			{
				((Control)iNN).Click -= eventHandler;
			}
			_INN = value;
			iNN = _INN;
			if (iNN != null)
			{
				((Control)iNN).Click += eventHandler;
			}
		}
	}

	internal virtual Button FIO
	{
		[CompilerGenerated]
		get
		{
			return _FIO;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = FIO_Click;
			Button fIO = _FIO;
			if (fIO != null)
			{
				((Control)fIO).Click -= eventHandler;
			}
			_FIO = value;
			fIO = _FIO;
			if (fIO != null)
			{
				((Control)fIO).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Logo")]
	internal virtual PictureBox Logo
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button InfaTaxPay
	{
		[CompilerGenerated]
		get
		{
			return _InfaTaxPay;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = InfaTaxPay_Click;
			Button infaTaxPay = _InfaTaxPay;
			if (infaTaxPay != null)
			{
				((Control)infaTaxPay).Click -= eventHandler;
			}
			_InfaTaxPay = value;
			infaTaxPay = _InfaTaxPay;
			if (infaTaxPay != null)
			{
				((Control)infaTaxPay).Click += eventHandler;
			}
		}
	}

	internal virtual Button IPN
	{
		[CompilerGenerated]
		get
		{
			return _IPN;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = IPN_Click;
			Button iPN = _IPN;
			if (iPN != null)
			{
				((Control)iPN).Click -= eventHandler;
			}
			_IPN = value;
			iPN = _IPN;
			if (iPN != null)
			{
				((Control)iPN).Click += eventHandler;
			}
		}
	}

	internal virtual Button EDP
	{
		[CompilerGenerated]
		get
		{
			return _EDP;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = EDP_Click;
			Button eDP = _EDP;
			if (eDP != null)
			{
				((Control)eDP).Click -= eventHandler;
			}
			_EDP = value;
			eDP = _EDP;
			if (eDP != null)
			{
				((Control)eDP).Click += eventHandler;
			}
		}
	}

	internal virtual Button TestPro
	{
		[CompilerGenerated]
		get
		{
			return _TestPro;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = TestPro_Click;
			Button testPro = _TestPro;
			if (testPro != null)
			{
				((Control)testPro).Click -= eventHandler;
			}
			_TestPro = value;
			testPro = _TestPro;
			if (testPro != null)
			{
				((Control)testPro).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("PrintDialog1")]
	internal virtual PrintDialog PrintDialog1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button ImportDat
	{
		[CompilerGenerated]
		get
		{
			return _ImportDat;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ImportDat_Click;
			Button importDat = _ImportDat;
			if (importDat != null)
			{
				((Control)importDat).Click -= eventHandler;
			}
			_ImportDat = value;
			importDat = _ImportDat;
			if (importDat != null)
			{
				((Control)importDat).Click += eventHandler;
			}
		}
	}

	internal virtual Button FNN
	{
		[CompilerGenerated]
		get
		{
			return _FNN;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = FNN_Click;
			Button fNN = _FNN;
			if (fNN != null)
			{
				((Control)fNN).Click -= eventHandler;
			}
			_FNN = value;
			fNN = _FNN;
			if (fNN != null)
			{
				((Control)fNN).Click += eventHandler;
			}
		}
	}

	internal virtual CheckBox CheckBoxManual
	{
		[CompilerGenerated]
		get
		{
			return _CheckBoxManual;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = CheckBoxManual_CheckedChanged;
			CheckBox checkBoxManual = _CheckBoxManual;
			if (checkBoxManual != null)
			{
				checkBoxManual.CheckedChanged -= eventHandler;
			}
			_CheckBoxManual = value;
			checkBoxManual = _CheckBoxManual;
			if (checkBoxManual != null)
			{
				checkBoxManual.CheckedChanged += eventHandler;
			}
		}
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_0053: Unknown result type (might be due to invalid IL or missing references)
		//IL_005d: Expected O, but got Unknown
		//IL_005e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0068: Expected O, but got Unknown
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		//IL_0073: Expected O, but got Unknown
		//IL_0074: Unknown result type (might be due to invalid IL or missing references)
		//IL_007e: Expected O, but got Unknown
		//IL_007f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0089: Expected O, but got Unknown
		//IL_008a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0094: Expected O, but got Unknown
		//IL_0095: Unknown result type (might be due to invalid IL or missing references)
		//IL_009f: Expected O, but got Unknown
		//IL_00a0: Unknown result type (might be due to invalid IL or missing references)
		//IL_00aa: Expected O, but got Unknown
		//IL_00ab: Unknown result type (might be due to invalid IL or missing references)
		//IL_00b5: Expected O, but got Unknown
		//IL_00b6: Unknown result type (might be due to invalid IL or missing references)
		//IL_00c0: Expected O, but got Unknown
		//IL_00c1: Unknown result type (might be due to invalid IL or missing references)
		//IL_00cb: Expected O, but got Unknown
		//IL_00cc: Unknown result type (might be due to invalid IL or missing references)
		//IL_00d6: Expected O, but got Unknown
		//IL_00d7: Unknown result type (might be due to invalid IL or missing references)
		//IL_00e1: Expected O, but got Unknown
		//IL_00e2: Unknown result type (might be due to invalid IL or missing references)
		//IL_00ec: Expected O, but got Unknown
		//IL_00ed: Unknown result type (might be due to invalid IL or missing references)
		//IL_00f7: Expected O, but got Unknown
		//IL_00f8: Unknown result type (might be due to invalid IL or missing references)
		//IL_0102: Expected O, but got Unknown
		//IL_0103: Unknown result type (might be due to invalid IL or missing references)
		//IL_010d: Expected O, but got Unknown
		//IL_010e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0118: Expected O, but got Unknown
		//IL_0119: Unknown result type (might be due to invalid IL or missing references)
		//IL_0123: Expected O, but got Unknown
		//IL_0124: Unknown result type (might be due to invalid IL or missing references)
		//IL_012e: Expected O, but got Unknown
		//IL_012f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0139: Expected O, but got Unknown
		//IL_013a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0144: Expected O, but got Unknown
		//IL_0145: Unknown result type (might be due to invalid IL or missing references)
		//IL_014f: Expected O, but got Unknown
		//IL_0150: Unknown result type (might be due to invalid IL or missing references)
		//IL_015a: Expected O, but got Unknown
		//IL_015b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0165: Expected O, but got Unknown
		//IL_0166: Unknown result type (might be due to invalid IL or missing references)
		//IL_0170: Expected O, but got Unknown
		//IL_0171: Unknown result type (might be due to invalid IL or missing references)
		//IL_017b: Expected O, but got Unknown
		//IL_017c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0186: Expected O, but got Unknown
		//IL_0187: Unknown result type (might be due to invalid IL or missing references)
		//IL_0191: Expected O, but got Unknown
		//IL_0192: Unknown result type (might be due to invalid IL or missing references)
		//IL_019c: Expected O, but got Unknown
		//IL_019d: Unknown result type (might be due to invalid IL or missing references)
		//IL_01a7: Expected O, but got Unknown
		//IL_01a8: Unknown result type (might be due to invalid IL or missing references)
		//IL_01b2: Expected O, but got Unknown
		//IL_01b3: Unknown result type (might be due to invalid IL or missing references)
		//IL_01bd: Expected O, but got Unknown
		//IL_01be: Unknown result type (might be due to invalid IL or missing references)
		//IL_01c8: Expected O, but got Unknown
		//IL_01c9: Unknown result type (might be due to invalid IL or missing references)
		//IL_01d3: Expected O, but got Unknown
		//IL_01d4: Unknown result type (might be due to invalid IL or missing references)
		//IL_01de: Expected O, but got Unknown
		//IL_01df: Unknown result type (might be due to invalid IL or missing references)
		//IL_01e9: Expected O, but got Unknown
		//IL_01ea: Unknown result type (might be due to invalid IL or missing references)
		//IL_01f4: Expected O, but got Unknown
		//IL_01f5: Unknown result type (might be due to invalid IL or missing references)
		//IL_01ff: Expected O, but got Unknown
		//IL_0200: Unknown result type (might be due to invalid IL or missing references)
		//IL_020a: Expected O, but got Unknown
		//IL_025f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0269: Expected O, but got Unknown
		//IL_0463: Unknown result type (might be due to invalid IL or missing references)
		//IL_046d: Expected O, but got Unknown
		//IL_04ea: Unknown result type (might be due to invalid IL or missing references)
		//IL_04f4: Expected O, but got Unknown
		//IL_056f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0579: Expected O, but got Unknown
		//IL_05f4: Unknown result type (might be due to invalid IL or missing references)
		//IL_05fe: Expected O, but got Unknown
		//IL_0679: Unknown result type (might be due to invalid IL or missing references)
		//IL_0683: Expected O, but got Unknown
		//IL_0701: Unknown result type (might be due to invalid IL or missing references)
		//IL_070b: Expected O, but got Unknown
		//IL_0789: Unknown result type (might be due to invalid IL or missing references)
		//IL_0793: Expected O, but got Unknown
		//IL_081d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0827: Expected O, but got Unknown
		//IL_08a1: Unknown result type (might be due to invalid IL or missing references)
		//IL_08ab: Expected O, but got Unknown
		//IL_091d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0927: Expected O, but got Unknown
		//IL_09a4: Unknown result type (might be due to invalid IL or missing references)
		//IL_09ae: Expected O, but got Unknown
		//IL_0a2b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0a35: Expected O, but got Unknown
		//IL_0aa7: Unknown result type (might be due to invalid IL or missing references)
		//IL_0ab1: Expected O, but got Unknown
		//IL_0b21: Unknown result type (might be due to invalid IL or missing references)
		//IL_0b2b: Expected O, but got Unknown
		//IL_0b9b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0ba5: Expected O, but got Unknown
		//IL_0c1e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0c28: Expected O, but got Unknown
		//IL_0c96: Unknown result type (might be due to invalid IL or missing references)
		//IL_0ca0: Expected O, but got Unknown
		//IL_0d31: Unknown result type (might be due to invalid IL or missing references)
		//IL_0d3b: Expected O, but got Unknown
		//IL_0da6: Unknown result type (might be due to invalid IL or missing references)
		//IL_0db0: Expected O, but got Unknown
		//IL_0f3d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0f47: Expected O, but got Unknown
		//IL_0fc7: Unknown result type (might be due to invalid IL or missing references)
		//IL_0fd1: Expected O, but got Unknown
		//IL_104f: Unknown result type (might be due to invalid IL or missing references)
		//IL_1059: Expected O, but got Unknown
		//IL_10d4: Unknown result type (might be due to invalid IL or missing references)
		//IL_10de: Expected O, but got Unknown
		//IL_11c8: Unknown result type (might be due to invalid IL or missing references)
		//IL_11d2: Expected O, but got Unknown
		//IL_1250: Unknown result type (might be due to invalid IL or missing references)
		//IL_125a: Expected O, but got Unknown
		//IL_12d2: Unknown result type (might be due to invalid IL or missing references)
		//IL_12dc: Expected O, but got Unknown
		//IL_134a: Unknown result type (might be due to invalid IL or missing references)
		//IL_1354: Expected O, but got Unknown
		//IL_13de: Unknown result type (might be due to invalid IL or missing references)
		//IL_13e8: Expected O, but got Unknown
		//IL_1456: Unknown result type (might be due to invalid IL or missing references)
		//IL_1460: Expected O, but got Unknown
		//IL_14ce: Unknown result type (might be due to invalid IL or missing references)
		//IL_14d8: Expected O, but got Unknown
		//IL_1552: Unknown result type (might be due to invalid IL or missing references)
		//IL_155c: Expected O, but got Unknown
		//IL_15ca: Unknown result type (might be due to invalid IL or missing references)
		//IL_15d4: Expected O, but got Unknown
		//IL_1660: Unknown result type (might be due to invalid IL or missing references)
		//IL_166a: Expected O, but got Unknown
		//IL_16e8: Unknown result type (might be due to invalid IL or missing references)
		//IL_16f2: Expected O, but got Unknown
		//IL_1807: Unknown result type (might be due to invalid IL or missing references)
		//IL_1811: Expected O, but got Unknown
		//IL_189e: Unknown result type (might be due to invalid IL or missing references)
		//IL_18a8: Expected O, but got Unknown
		//IL_193f: Unknown result type (might be due to invalid IL or missing references)
		//IL_1949: Expected O, but got Unknown
		//IL_19c7: Unknown result type (might be due to invalid IL or missing references)
		//IL_19d1: Expected O, but got Unknown
		//IL_1a58: Unknown result type (might be due to invalid IL or missing references)
		//IL_1a62: Expected O, but got Unknown
		//IL_1ad9: Unknown result type (might be due to invalid IL or missing references)
		//IL_1ae3: Expected O, but got Unknown
		//IL_1b4d: Unknown result type (might be due to invalid IL or missing references)
		//IL_1b57: Expected O, but got Unknown
		//IL_1c3b: Unknown result type (might be due to invalid IL or missing references)
		//IL_1c45: Expected O, but got Unknown
		//IL_1d84: Unknown result type (might be due to invalid IL or missing references)
		//IL_1d8e: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormNewPro));
		Label1 = new Label();
		GroupBox1 = new GroupBox();
		FNN = new Button();
		IPN = new Button();
		EDP = new Button();
		Adr = new Button();
		NamT = new Button();
		NamO = new Button();
		Label4 = new Label();
		Label7 = new Label();
		AtorgT = new TextBox();
		Label5 = new Label();
		Label6 = new Label();
		NtorgT = new TextBox();
		NorgT = new TextBox();
		InnT = new TextBox();
		Label3 = new Label();
		FnT = new TextBox();
		Label2 = new Label();
		TinT = new TextBox();
		GroupBox2 = new GroupBox();
		Pas = new Button();
		INN = new Button();
		FIO = new Button();
		KeyB = new Button();
		Label11 = new Label();
		Label10 = new Label();
		Label9 = new Label();
		PasOpT = new TextBox();
		KeyOpT = new TextBox();
		InnOpT = new TextBox();
		FioOpT = new TextBox();
		Label8 = new Label();
		OkB = new Button();
		CheckBoxTest = new CheckBox();
		SelSwrver = new Button();
		GroupBox3 = new GroupBox();
		CheckBoxManual = new CheckBox();
		ImportDat = new Button();
		TestPro = new Button();
		Server = new TextBox();
		Label21 = new Label();
		NoB = new Button();
		Logo = new PictureBox();
		InfaTaxPay = new Button();
		PrintDialog1 = new PrintDialog();
		((Control)GroupBox1).SuspendLayout();
		((Control)GroupBox2).SuspendLayout();
		((Control)GroupBox3).SuspendLayout();
		((ISupportInitialize)Logo).BeginInit();
		((Control)this).SuspendLayout();
		Label1.AutoSize = true;
		((Control)Label1).Font = new Font("Microsoft Sans Serif", 16.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label1).Location = new Point(18, 9);
		((Control)Label1).Name = "Label1";
		((Control)Label1).Size = new Size(476, 32);
		((Control)Label1).TabIndex = 0;
		Label1.Text = "Майстер заповнення нового ПРРО";
		((Control)GroupBox1).Controls.Add((Control)(object)FNN);
		((Control)GroupBox1).Controls.Add((Control)(object)IPN);
		((Control)GroupBox1).Controls.Add((Control)(object)EDP);
		((Control)GroupBox1).Controls.Add((Control)(object)Adr);
		((Control)GroupBox1).Controls.Add((Control)(object)NamT);
		((Control)GroupBox1).Controls.Add((Control)(object)NamO);
		((Control)GroupBox1).Controls.Add((Control)(object)Label4);
		((Control)GroupBox1).Controls.Add((Control)(object)Label7);
		((Control)GroupBox1).Controls.Add((Control)(object)AtorgT);
		((Control)GroupBox1).Controls.Add((Control)(object)Label5);
		((Control)GroupBox1).Controls.Add((Control)(object)Label6);
		((Control)GroupBox1).Controls.Add((Control)(object)NtorgT);
		((Control)GroupBox1).Controls.Add((Control)(object)NorgT);
		((Control)GroupBox1).Controls.Add((Control)(object)InnT);
		((Control)GroupBox1).Controls.Add((Control)(object)Label3);
		((Control)GroupBox1).Controls.Add((Control)(object)FnT);
		((Control)GroupBox1).Controls.Add((Control)(object)Label2);
		((Control)GroupBox1).Controls.Add((Control)(object)TinT);
		((Control)GroupBox1).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)GroupBox1).Location = new Point(12, 55);
		((Control)GroupBox1).Name = "GroupBox1";
		((Control)GroupBox1).Size = new Size(660, 271);
		((Control)GroupBox1).TabIndex = 0;
		GroupBox1.TabStop = false;
		GroupBox1.Text = "Організація";
		((Control)FNN).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)FNN).Location = new Point(588, 65);
		((Control)FNN).Name = "FNN";
		((Control)FNN).Size = new Size(53, 30);
		((Control)FNN).TabIndex = 26;
		((ButtonBase)FNN).Text = "...";
		((ButtonBase)FNN).UseVisualStyleBackColor = true;
		((Control)IPN).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)IPN).Location = new Point(588, 107);
		((Control)IPN).Name = "IPN";
		((Control)IPN).Size = new Size(53, 30);
		((Control)IPN).TabIndex = 25;
		((ButtonBase)IPN).Text = "...";
		((ButtonBase)IPN).UseVisualStyleBackColor = true;
		((Control)EDP).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)EDP).Location = new Point(588, 26);
		((Control)EDP).Name = "EDP";
		((Control)EDP).Size = new Size(53, 30);
		((Control)EDP).TabIndex = 24;
		((ButtonBase)EDP).Text = "...";
		((ButtonBase)EDP).UseVisualStyleBackColor = true;
		((Control)Adr).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Adr).Location = new Point(588, 226);
		((Control)Adr).Name = "Adr";
		((Control)Adr).Size = new Size(53, 30);
		((Control)Adr).TabIndex = 25;
		((ButtonBase)Adr).Text = "...";
		((ButtonBase)Adr).UseVisualStyleBackColor = true;
		((Control)NamT).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NamT).Location = new Point(588, 190);
		((Control)NamT).Name = "NamT";
		((Control)NamT).Size = new Size(53, 30);
		((Control)NamT).TabIndex = 24;
		((ButtonBase)NamT).Text = "...";
		((ButtonBase)NamT).UseVisualStyleBackColor = true;
		((Control)NamO).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NamO).Location = new Point(588, 150);
		((Control)NamO).Name = "NamO";
		((Control)NamO).Size = new Size(53, 30);
		((Control)NamO).TabIndex = 23;
		((ButtonBase)NamO).Text = "...";
		((ButtonBase)NamO).UseVisualStyleBackColor = true;
		Label4.AutoSize = true;
		((Control)Label4).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label4).Location = new Point(9, 110);
		((Control)Label4).Name = "Label4";
		((Control)Label4).Size = new Size(184, 25);
		((Control)Label4).TabIndex = 7;
		Label4.Text = "ІПН платника ПДВ";
		Label7.AutoSize = true;
		((Control)Label7).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label7).Location = new Point(9, 230);
		((Control)Label7).Name = "Label7";
		((Control)Label7).Size = new Size(237, 25);
		((Control)Label7).TabIndex = 11;
		Label7.Text = "Адреса торгової точки *";
		((Control)AtorgT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)AtorgT).Location = new Point(270, 227);
		((Control)AtorgT).Name = "AtorgT";
		((Control)AtorgT).Size = new Size(309, 30);
		((Control)AtorgT).TabIndex = 10;
		AtorgT.TextAlign = (HorizontalAlignment)2;
		Label5.AutoSize = true;
		((Control)Label5).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label5).Location = new Point(9, 150);
		((Control)Label5).Name = "Label5";
		((Control)Label5).Size = new Size(182, 25);
		((Control)Label5).TabIndex = 8;
		Label5.Text = "Назва організації *";
		Label6.AutoSize = true;
		((Control)Label6).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label6).Location = new Point(9, 190);
		((Control)Label6).Name = "Label6";
		((Control)Label6).Size = new Size(224, 25);
		((Control)Label6).TabIndex = 9;
		Label6.Text = "Назва торгової точки *";
		((Control)NtorgT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NtorgT).Location = new Point(270, 187);
		((Control)NtorgT).Name = "NtorgT";
		((Control)NtorgT).Size = new Size(309, 30);
		((Control)NtorgT).TabIndex = 6;
		NtorgT.TextAlign = (HorizontalAlignment)2;
		((Control)NorgT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NorgT).Location = new Point(270, 147);
		((Control)NorgT).Name = "NorgT";
		((Control)NorgT).Size = new Size(309, 30);
		((Control)NorgT).TabIndex = 5;
		NorgT.TextAlign = (HorizontalAlignment)2;
		((Control)InnT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)InnT).Location = new Point(270, 107);
		((Control)InnT).Name = "InnT";
		((Control)InnT).Size = new Size(309, 30);
		((Control)InnT).TabIndex = 4;
		InnT.TextAlign = (HorizontalAlignment)2;
		Label3.AutoSize = true;
		((Control)Label3).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label3).Location = new Point(9, 70);
		((Control)Label3).Name = "Label3";
		((Control)Label3).Size = new Size(199, 25);
		((Control)Label3).TabIndex = 3;
		Label3.Text = "Фіскальний номер *";
		((Control)FnT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)FnT).Location = new Point(270, 67);
		((Control)FnT).Name = "FnT";
		((TextBoxBase)FnT).ReadOnly = true;
		((Control)FnT).Size = new Size(309, 30);
		((Control)FnT).TabIndex = 2;
		((Control)FnT).TabStop = false;
		FnT.TextAlign = (HorizontalAlignment)2;
		Label2.AutoSize = true;
		((Control)Label2).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label2).Location = new Point(9, 29);
		((Control)Label2).Name = "Label2";
		((Control)Label2).Size = new Size(108, 25);
		((Control)Label2).TabIndex = 1;
		Label2.Text = "ЕДРПОУ *";
		((Control)TinT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TinT).Location = new Point(270, 27);
		((Control)TinT).Name = "TinT";
		((TextBoxBase)TinT).ReadOnly = true;
		((Control)TinT).Size = new Size(309, 30);
		((Control)TinT).TabIndex = 0;
		((Control)TinT).TabStop = false;
		TinT.TextAlign = (HorizontalAlignment)2;
		((Control)GroupBox2).Controls.Add((Control)(object)Pas);
		((Control)GroupBox2).Controls.Add((Control)(object)INN);
		((Control)GroupBox2).Controls.Add((Control)(object)FIO);
		((Control)GroupBox2).Controls.Add((Control)(object)KeyB);
		((Control)GroupBox2).Controls.Add((Control)(object)Label11);
		((Control)GroupBox2).Controls.Add((Control)(object)Label10);
		((Control)GroupBox2).Controls.Add((Control)(object)Label9);
		((Control)GroupBox2).Controls.Add((Control)(object)PasOpT);
		((Control)GroupBox2).Controls.Add((Control)(object)KeyOpT);
		((Control)GroupBox2).Controls.Add((Control)(object)InnOpT);
		((Control)GroupBox2).Controls.Add((Control)(object)FioOpT);
		((Control)GroupBox2).Controls.Add((Control)(object)Label8);
		((Control)GroupBox2).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)GroupBox2).Location = new Point(12, 332);
		((Control)GroupBox2).Name = "GroupBox2";
		((Control)GroupBox2).Size = new Size(660, 201);
		((Control)GroupBox2).TabIndex = 1;
		GroupBox2.TabStop = false;
		GroupBox2.Text = "Оператор";
		((Control)Pas).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Pas).Location = new Point(588, 153);
		((Control)Pas).Name = "Pas";
		((Control)Pas).Size = new Size(53, 30);
		((Control)Pas).TabIndex = 24;
		((ButtonBase)Pas).Text = "...";
		((ButtonBase)Pas).UseVisualStyleBackColor = true;
		((Control)INN).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)INN).Location = new Point(588, 73);
		((Control)INN).Name = "INN";
		((Control)INN).Size = new Size(53, 30);
		((Control)INN).TabIndex = 23;
		((ButtonBase)INN).Text = "...";
		((ButtonBase)INN).UseVisualStyleBackColor = true;
		((Control)FIO).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)FIO).Location = new Point(588, 32);
		((Control)FIO).Name = "FIO";
		((Control)FIO).Size = new Size(53, 30);
		((Control)FIO).TabIndex = 22;
		((ButtonBase)FIO).Text = "...";
		((ButtonBase)FIO).UseVisualStyleBackColor = true;
		((Control)KeyB).Location = new Point(588, 113);
		((Control)KeyB).Name = "KeyB";
		((Control)KeyB).Size = new Size(53, 30);
		((Control)KeyB).TabIndex = 0;
		((ButtonBase)KeyB).Text = "...";
		((ButtonBase)KeyB).UseVisualStyleBackColor = true;
		Label11.AutoSize = true;
		((Control)Label11).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label11).Location = new Point(9, 156);
		((Control)Label11).Name = "Label11";
		((Control)Label11).Size = new Size(202, 25);
		((Control)Label11).TabIndex = 18;
		Label11.Text = "Пароль ключа ЕЦП *";
		Label10.AutoSize = true;
		((Control)Label10).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label10).Location = new Point(9, 116);
		((Control)Label10).Name = "Label10";
		((Control)Label10).Size = new Size(121, 25);
		((Control)Label10).TabIndex = 17;
		Label10.Text = "Ключ ЕЦП *";
		Label9.AutoSize = true;
		((Control)Label9).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label9).Location = new Point(9, 76);
		((Control)Label9).Name = "Label9";
		((Control)Label9).Size = new Size(159, 25);
		((Control)Label9).TabIndex = 8;
		Label9.Text = "ІНН оператора *";
		((Control)PasOpT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)PasOpT).Location = new Point(270, 153);
		((Control)PasOpT).Name = "PasOpT";
		PasOpT.PasswordChar = '*';
		((Control)PasOpT).Size = new Size(309, 30);
		((Control)PasOpT).TabIndex = 16;
		PasOpT.TextAlign = (HorizontalAlignment)2;
		((Control)KeyOpT).Enabled = false;
		((Control)KeyOpT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)KeyOpT).Location = new Point(270, 113);
		((Control)KeyOpT).Name = "KeyOpT";
		((Control)KeyOpT).Size = new Size(309, 30);
		((Control)KeyOpT).TabIndex = 15;
		KeyOpT.TextAlign = (HorizontalAlignment)2;
		((Control)InnOpT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)InnOpT).Location = new Point(270, 73);
		((Control)InnOpT).Name = "InnOpT";
		((Control)InnOpT).Size = new Size(309, 30);
		((Control)InnOpT).TabIndex = 14;
		InnOpT.TextAlign = (HorizontalAlignment)2;
		((Control)FioOpT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)FioOpT).Location = new Point(270, 32);
		((Control)FioOpT).Name = "FioOpT";
		((Control)FioOpT).Size = new Size(309, 30);
		((Control)FioOpT).TabIndex = 13;
		FioOpT.TextAlign = (HorizontalAlignment)2;
		Label8.AutoSize = true;
		((Control)Label8).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label8).Location = new Point(9, 35);
		((Control)Label8).Name = "Label8";
		((Control)Label8).Size = new Size(159, 25);
		((Control)Label8).TabIndex = 2;
		Label8.Text = "ПІБ оператора *";
		((Control)OkB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OkB).Location = new Point(912, 493);
		((Control)OkB).Name = "OkB";
		((Control)OkB).Size = new Size(132, 40);
		((Control)OkB).TabIndex = 4;
		((ButtonBase)OkB).Text = "Створити";
		((ButtonBase)OkB).UseVisualStyleBackColor = true;
		((ButtonBase)CheckBoxTest).AutoSize = true;
		((Control)CheckBoxTest).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)CheckBoxTest).Location = new Point(22, 249);
		((Control)CheckBoxTest).Name = "CheckBoxTest";
		((Control)CheckBoxTest).Size = new Size(299, 24);
		((Control)CheckBoxTest).TabIndex = 18;
		((ButtonBase)CheckBoxTest).Text = "Заповнити даними для тестів...";
		((ButtonBase)CheckBoxTest).UseVisualStyleBackColor = true;
		((Control)SelSwrver).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)SelSwrver).Location = new Point(288, 40);
		((Control)SelSwrver).Name = "SelSwrver";
		((Control)SelSwrver).Size = new Size(53, 30);
		((Control)SelSwrver).TabIndex = 19;
		((ButtonBase)SelSwrver).Text = "...";
		((ButtonBase)SelSwrver).UseVisualStyleBackColor = true;
		((Control)GroupBox3).Controls.Add((Control)(object)CheckBoxManual);
		((Control)GroupBox3).Controls.Add((Control)(object)ImportDat);
		((Control)GroupBox3).Controls.Add((Control)(object)TestPro);
		((Control)GroupBox3).Controls.Add((Control)(object)Server);
		((Control)GroupBox3).Controls.Add((Control)(object)SelSwrver);
		((Control)GroupBox3).Controls.Add((Control)(object)Label21);
		((Control)GroupBox3).Controls.Add((Control)(object)CheckBoxTest);
		((Control)GroupBox3).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)GroupBox3).Location = new Point(690, 126);
		((Control)GroupBox3).Name = "GroupBox3";
		((Control)GroupBox3).Size = new Size(354, 349);
		((Control)GroupBox3).TabIndex = 19;
		GroupBox3.TabStop = false;
		GroupBox3.Text = "Додатковe налаштування";
		((ButtonBase)CheckBoxManual).AutoSize = true;
		((Control)CheckBoxManual).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)CheckBoxManual).Location = new Point(22, 288);
		((Control)CheckBoxManual).Name = "CheckBoxManual";
		((Control)CheckBoxManual).Size = new Size(319, 44);
		((Control)CheckBoxManual).TabIndex = 24;
		((Control)CheckBoxManual).TabStop = false;
		((ButtonBase)CheckBoxManual).Text = "Ручне заповнення даних. Увага! \r\n(Без відновлення резервної копії)";
		CheckBoxManual.TextAlign = (ContentAlignment)32;
		((ButtonBase)CheckBoxManual).UseVisualStyleBackColor = true;
		((Control)ImportDat).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)ImportDat).Location = new Point(11, 144);
		((Control)ImportDat).Name = "ImportDat";
		((Control)ImportDat).Size = new Size(330, 78);
		((Control)ImportDat).TabIndex = 23;
		((ButtonBase)ImportDat).Text = "Завантаження даних з кабінету податкової...";
		((ButtonBase)ImportDat).UseVisualStyleBackColor = true;
		((Control)TestPro).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TestPro).Location = new Point(11, 91);
		((Control)TestPro).Name = "TestPro";
		((Control)TestPro).Size = new Size(330, 38);
		((Control)TestPro).TabIndex = 21;
		((ButtonBase)TestPro).Text = "Перевірка налаштувань...";
		((ButtonBase)TestPro).UseVisualStyleBackColor = true;
		((Control)Server).Enabled = false;
		((Control)Server).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Server).Location = new Point(76, 40);
		((Control)Server).Name = "Server";
		((Control)Server).Size = new Size(202, 30);
		((Control)Server).TabIndex = 20;
		Server.TextAlign = (HorizontalAlignment)2;
		Label21.AutoSize = true;
		((Control)Label21).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label21).Location = new Point(6, 43);
		((Control)Label21).Name = "Label21";
		((Control)Label21).Size = new Size(64, 25);
		((Control)Label21).TabIndex = 8;
		Label21.Text = "АЦСК";
		((Control)NoB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NoB).Location = new Point(690, 493);
		((Control)NoB).Name = "NoB";
		((Control)NoB).Size = new Size(132, 40);
		((Control)NoB).TabIndex = 20;
		((ButtonBase)NoB).Text = "Скасувати";
		((ButtonBase)NoB).UseVisualStyleBackColor = true;
		((Control)Logo).Location = new Point(690, 12);
		((Control)Logo).Name = "Logo";
		((Control)Logo).Size = new Size(354, 108);
		Logo.SizeMode = (PictureBoxSizeMode)4;
		Logo.TabIndex = 21;
		Logo.TabStop = false;
		((Control)InfaTaxPay).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)InfaTaxPay).Location = new Point(540, 12);
		((Control)InfaTaxPay).Name = "InfaTaxPay";
		((Control)InfaTaxPay).Size = new Size(132, 40);
		((Control)InfaTaxPay).TabIndex = 21;
		((ButtonBase)InfaTaxPay).Text = "Iнфо";
		((ButtonBase)InfaTaxPay).UseVisualStyleBackColor = true;
		PrintDialog1.UseEXDialog = true;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(1063, 544);
		((Control)this).Controls.Add((Control)(object)InfaTaxPay);
		((Control)this).Controls.Add((Control)(object)NoB);
		((Control)this).Controls.Add((Control)(object)GroupBox3);
		((Control)this).Controls.Add((Control)(object)OkB);
		((Control)this).Controls.Add((Control)(object)GroupBox2);
		((Control)this).Controls.Add((Control)(object)GroupBox1);
		((Control)this).Controls.Add((Control)(object)Label1);
		((Control)this).Controls.Add((Control)(object)Logo);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormNewPro";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Новий ПРРО";
		((Control)GroupBox1).ResumeLayout(false);
		((Control)GroupBox1).PerformLayout();
		((Control)GroupBox2).ResumeLayout(false);
		((Control)GroupBox2).PerformLayout();
		((Control)GroupBox3).ResumeLayout(false);
		((Control)GroupBox3).PerformLayout();
		((ISupportInitialize)Logo).EndInit();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	public FormNewPro(string FnStr, string OperatorID, bool DemoInfa = false, bool DemoOnly = false)
	{
		((Form)this).Load += FormNewPro_Load;
		((Form)this).Closing += FormNewPro_Closing;
		ParOld = "";
		InitializeComponent();
		All.A.AcskSettingsTemp = 0;
		if (DemoOnly)
		{
			CheckBoxTest.Checked = true;
			((Control)CheckBoxTest).Enabled = false;
			Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
			((Control)InfaTaxPay).Enabled = false;
			((Control)SelSwrver).Enabled = false;
			((Control)ImportDat).Enabled = false;
		}
		else if (DemoInfa)
		{
			((Control)ImportDat).Enabled = false;
			NewBase = false;
			((Form)this).Text = "Data RRO ";
			Label1.Text = "Основна інформація:";
			((Control)CheckBoxTest).Visible = false;
			((Control)OkB).Visible = false;
			((ButtonBase)NoB).Text = "Закрити";
			((Control)NoB).Left = ((Control)OkB).Left;
			BlokText(blok: true);
			TinT.Text = All.A.TIN;
			FnT.Text = All.A.FN;
			InnT.Text = All.A.INN;
			NorgT.Text = All.A.OrgName;
			NtorgT.Text = All.A.PointName;
			AtorgT.Text = All.A.PointAddr;
			Server.Text = All.SF.Servers(All.A.AcskSettings).Name;
			OperatorsAll operatorsAll = new OperatorsAll();
			if (operatorsAll.Operators > 0)
			{
				int y = 1;
				FioOpT.Text = operatorsAll.get_Seller(1, y);
				KeyOpT.Text = operatorsAll.get_Seller(2, y);
				PasOpT.Text = "*********";
				InnOpT.Text = operatorsAll.get_Seller(4, y);
				Coding coding = new Coding();
				ParOld = coding.DeCod(operatorsAll.get_Seller(3, y));
			}
			if ((FnStr.Length == 10) & Versioned.IsNumeric((object)FnStr))
			{
				((Control)CheckBoxTest).Enabled = false;
				((Control)FnT).Enabled = false;
				FnT.Text = FnStr;
			}
			if (OperatorID.Trim().Length > 0)
			{
				((Control)CheckBoxTest).Enabled = false;
				((Control)InnOpT).Enabled = false;
				InnOpT.Text = OperatorID;
			}
		}
		else
		{
			((Control)ImportDat).Enabled = true;
			NewBase = true;
			((Control)InfaTaxPay).Enabled = false;
			BlokText(blok: false);
			Server.Text = "";
		}
	}

	private void Zupolnit()
	{
		TinT.Text = All.A.TIN;
		FnT.Text = All.A.FN;
		InnT.Text = All.A.INN;
		NorgT.Text = All.A.OrgName;
		NtorgT.Text = All.A.PointName;
		AtorgT.Text = All.A.PointAddr;
		Server.Text = All.SF.Servers(All.A.AcskSettings).Name;
		OperatorsAll operatorsAll = new OperatorsAll();
		if (operatorsAll.Operators > 0)
		{
			FioOpT.Text = operatorsAll.get_Seller(1, 1);
			KeyOpT.Text = operatorsAll.get_Seller(2, 1);
			PasOpT.Text = "*********";
			InnOpT.Text = operatorsAll.get_Seller(4, 1);
		}
	}

	private void FormNewPro_Load(object sender, EventArgs e)
	{
		((Form)this).AcceptButton = (IButtonControl)(object)OkB;
		((Form)this).CancelButton = (IButtonControl)(object)NoB;
		string text = All.MyDoc() + "\\WebCheck\\logo.gif";
		if (File.Exists(text))
		{
			Image image = Image.FromFile(text);
			Logo.Image = image;
		}
		else
		{
			text = All.MyDoc() + "\\WebCheck\\logo.jpg";
			if (File.Exists(text))
			{
				Image image = Image.FromFile(text);
				Logo.Image = image;
			}
			else
			{
				text = All.MyDoc() + "\\WebCheck\\logo.png";
				if (File.Exists(text))
				{
					Image image = Image.FromFile(text);
					Logo.Image = image;
				}
			}
		}
		Application.DoEvents();
	}

	private bool CreateTables(string FnS)
	{
		CreateDB createDB = new CreateDB(FnS);
		int num = 0;
		do
		{
			createDB.CreateTable(num);
			Application.DoEvents();
			num = checked(num + 1);
		}
		while (num <= 13);
		createDB.CreateTriger();
		createDB.CreateTriger1();
		createDB.CreateTriger2();
		createDB.CreateTrigerBackup();
		createDB.CreateIndex(newPRO: true);
		return true;
	}

	private void BlokText(bool blok)
	{
		if (blok)
		{
			((Control)TinT).Enabled = false;
			((Control)FnT).Enabled = false;
			((Control)InnT).Enabled = false;
			((Control)NorgT).Enabled = false;
			((Control)NtorgT).Enabled = false;
			((Control)AtorgT).Enabled = false;
			((Control)FioOpT).Enabled = false;
			((Control)InnOpT).Enabled = false;
			((Control)PasOpT).Enabled = false;
			((Control)EDP).Enabled = false;
			((Control)FNN).Enabled = false;
			((Control)IPN).Enabled = true;
			((Control)NamO).Enabled = true;
			((Control)NamT).Enabled = true;
			((Control)Adr).Enabled = true;
			((Control)FIO).Enabled = true;
			((Control)INN).Enabled = true;
			((Control)KeyB).Enabled = true;
			((Control)Pas).Enabled = true;
			((Control)SelSwrver).Enabled = true;
			((Control)TestPro).Enabled = true;
			((Control)CheckBoxManual).Enabled = false;
		}
		else
		{
			((Control)TinT).Enabled = true;
			((Control)FnT).Enabled = true;
			((Control)InnT).Enabled = true;
			((Control)NorgT).Enabled = true;
			((Control)NtorgT).Enabled = true;
			((Control)AtorgT).Enabled = true;
			((Control)FioOpT).Enabled = true;
			((Control)InnOpT).Enabled = true;
			((Control)PasOpT).Enabled = true;
			((Control)EDP).Enabled = true;
			((Control)FNN).Enabled = true;
			((Control)IPN).Enabled = false;
			((Control)NamO).Enabled = false;
			((Control)NamT).Enabled = false;
			((Control)Adr).Enabled = false;
			((Control)FIO).Enabled = false;
			((Control)INN).Enabled = false;
			((Control)KeyB).Enabled = true;
			((Control)Pas).Enabled = false;
			((Control)SelSwrver).Enabled = true;
			((Control)TestPro).Enabled = false;
			((Control)CheckBoxManual).Enabled = true;
		}
	}

	public bool CreateRow(string FnS, string NewPar = "")
	{
		CreateDB createDB = new CreateDB(FnS);
		createDB.SaveTaxObjects(FnT.Text, TinT.Text, InnT.Text, All.l.TextToTextSQL(NtorgT.Text), All.l.TextToTextSQL(NorgT.Text), All.l.TextToTextSQL(AtorgT.Text));
		createDB.SaveOperators(PassS: (Operators.CompareString(NewPar.Trim(), "", false) == 0) ? PasOpT.Text : NewPar, FioS: All.l.TextToTextXML(FioOpT.Text), PathKS: KeyOpT.Text, InnS: InnOpT.Text);
		Application.DoEvents();
		int num = 1;
		do
		{
			createDB.SaveInfoTable(num);
			Application.DoEvents();
			num = checked(num + 1);
		}
		while (num <= 13);
		Application.DoEvents();
		return true;
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		//IL_0084: Unknown result type (might be due to invalid IL or missing references)
		//IL_00b5: Unknown result type (might be due to invalid IL or missing references)
		//IL_027f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0537: Unknown result type (might be due to invalid IL or missing references)
		//IL_0304: Unknown result type (might be due to invalid IL or missing references)
		//IL_030a: Invalid comparison between Unknown and I4
		//IL_051e: Unknown result type (might be due to invalid IL or missing references)
		if (CheckBoxManual.Checked)
		{
			if (Operators.CompareString(TinT.Text.Trim(), "", false) == 0)
			{
				((Control)TinT).Focus();
				return;
			}
			if (Operators.CompareString(FnT.Text.Trim(), "", false) == 0)
			{
				((Control)FnT).Focus();
				return;
			}
			if (FnT.Text.Length != 10)
			{
				Interaction.MsgBox((object)"Не вірний формат фіскального номера!", (MsgBoxStyle)48, (object)"Новий PRO");
				((Control)FnT).Focus();
				return;
			}
			if (!Versioned.IsNumeric((object)FnT.Text))
			{
				Interaction.MsgBox((object)"Не вірний формат фіскального номера!", (MsgBoxStyle)48, (object)"Новий PRO");
				((Control)FnT).Focus();
				return;
			}
		}
		if ((Operators.CompareString(TinT.Text.Trim(), "", false) == 0) | (Operators.CompareString(FnT.Text.Trim(), "", false) == 0))
		{
			ImportDat.PerformClick();
		}
		if (Operators.CompareString(NorgT.Text.Trim(), "", false) == 0)
		{
			((Control)NorgT).Focus();
			return;
		}
		if (Operators.CompareString(NtorgT.Text.Trim(), "", false) == 0)
		{
			((Control)NtorgT).Focus();
			return;
		}
		if (Operators.CompareString(AtorgT.Text.Trim(), "", false) == 0)
		{
			((Control)AtorgT).Focus();
			return;
		}
		if (Operators.CompareString(FioOpT.Text.Trim(), "", false) == 0)
		{
			((Control)FioOpT).Focus();
			return;
		}
		if (Operators.CompareString(InnOpT.Text.Trim(), "", false) == 0)
		{
			((Control)InnOpT).Focus();
			return;
		}
		if (Operators.CompareString(KeyOpT.Text.Trim(), "", false) == 0)
		{
			string text = PathKey();
			if (Operators.CompareString(text, "", false) != 0)
			{
				KeyOpT.Text = text;
				((Control)PasOpT).Focus();
			}
			return;
		}
		if (Operators.CompareString(PasOpT.Text.Trim(), "", false) == 0)
		{
			((Control)PasOpT).Focus();
			return;
		}
		if (Operators.CompareString(Server.Text, "", false) == 0)
		{
			((Form)new FormServerSelection(NewBase)).ShowDialog();
			Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
		}
		string text2 = All.MyDoc() + "\\WebCheck\\DB\\" + FnT.Text + ".db";
		string text3 = "";
		if (TestBackup(text2))
		{
			text3 = "Резервна база встановлена і підключена!";
		}
		if (All.f.IndexFn(FnT.Text) == 0)
		{
			if ((int)Interaction.MsgBox((object)"Створити нову базу даних?", (MsgBoxStyle)33, (object)"Новий PRO") == 1)
			{
				All.f.AddFn(FnT.Text);
				All.f.StringWriteFN(FnT.Text, "Path", text2);
				All.f.StringWriteFN(FnT.Text, "TIN", TinT.Text);
				All.f.StringWriteFN(FnT.Text, "On", "1");
				All.f.StringWriteFN(FnT.Text, "Save", "0");
				All.f.StringWriteFN(FnT.Text, "ShowPintForm", "1");
				All.f.StringWriteFN(FnT.Text, "LogOn", "1");
				All.f.StringWriteFN(All.A.FN, "Acsksettings", All.A.AcskSettingsTemp.ToString());
				if (File.Exists(text2) && Operators.CompareString(text3, "", false) == 0)
				{
					text3 = "База підключена!";
				}
				((Control)OkB).Enabled = false;
				((Control)this).Enabled = false;
				string text4 = FnT.Text.Trim();
				CreateTables(text4);
				CreateRow(text4);
				CopyINI(text4);
				text4 += "_TS";
				CreateTables(text4);
				CreateRow(text4);
				All.NewFolderFn();
				All.A.AcskSettings = All.f.IntegerGetFn(All.A.FN, "Acsksettings");
				if (Operators.CompareString(All.f.StringGetFn(All.A.FN, "Acsksettings"), "", false) == 0)
				{
					All.f.StringWriteFN(All.A.FN, "Acsksettings", All.A.AcskSettingsTemp.ToString());
				}
				if (Operators.CompareString(text3, "", false) == 0)
				{
					StartBackup(text2);
					text3 = "База успішно створена!";
				}
				Interaction.MsgBox((object)text3, (MsgBoxStyle)64, (object)"Новий PRO");
				((Form)this).Close();
			}
		}
		else
		{
			Interaction.MsgBox((object)"Такий FN вже є!", (MsgBoxStyle)48, (object)"Новий PRO");
		}
	}

	private void StartBackup(string PathDB)
	{
		string fN = All.A.FN;
		string fileN = All.A.FileN;
		string connection = All.A.Connection;
		All.A.FN = FnT.Text.Trim();
		All.A.FileN = PathDB;
		All.A.Connection = "Data Source=" + All.A.FileN + "; Version=3";
		CreateDB createDB = new CreateDB(All.A.FN);
		createDB.CreateTable(13);
		createDB.CreateTrigerBackup();
		string fileN2 = All.A.FileN;
		string text = All.MyDoc() + "\\WebCheck\\Backup\\" + All.A.FN + ".db";
		try
		{
			if (!File.Exists(text))
			{
				File.Copy(fileN2, text);
				Application.DoEvents();
				All.l.ClearBackups();
				Application.DoEvents();
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
		All.A.FN = fN;
		All.A.FileN = fileN;
		All.A.Connection = connection;
	}

	private bool TestBackup(string PathN)
	{
		bool result;
		if (File.Exists(PathN))
		{
			result = false;
		}
		else
		{
			string text = All.MyDoc() + "\\WebCheck\\Backup\\" + FnT.Text + ".db";
			if (!File.Exists(text) && innKeyTrue())
			{
				string text2 = All.PersonalTemp() + "s3.txt";
				if (!File.Exists(text2))
				{
					DownLoadFileS3(text2);
				}
				Coding coding = new Coding();
				IniHGB iniHGB = new IniHGB(text2);
				string keyId = coding.DeCod(iniHGB.GetString("AWS", "KeyId"));
				string secret = coding.DeCod(iniHGB.GetString("AWS", "Secret"));
				WriteProfile(keyId, secret);
				string f = FnT.Text.Trim();
				string t = TinT.Text.Trim();
				NP nP = default(NP);
				if (!nP.FileArchive(ref f, ref t))
				{
					result = false;
					goto IL_0149;
				}
				if (DownLoadZip(f, t))
				{
					f = All.MyDoc() + "\\WebCheck\\Backup\\" + f + ".zip";
					if (File.Exists(f))
					{
						FileSystem.DeleteFile(f);
					}
				}
			}
			if (File.Exists(text))
			{
				try
				{
					File.Copy(text, PathN);
					result = true;
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					result = false;
					ProjectData.ClearProjectError();
				}
			}
			else
			{
				result = false;
			}
		}
		goto IL_0149;
		IL_0149:
		return result;
	}

	internal bool DownLoadZip(string Name, string Key)
	{
		AmazonS3Client amazonS3Client = new AmazonS3Client(RegionEndpoint.EUWest2);
		if (!Directory.Exists(All.MyDoc() + "\\WebCheck\\Backup\\"))
		{
			Directory.CreateDirectory(All.MyDoc() + "\\WebCheck\\Backup\\");
		}
		GetObjectRequest getObjectRequest = new GetObjectRequest();
		getObjectRequest.BucketName = "webchekzipfns";
		getObjectRequest.Key = Name + ".zip";
		_ = null;
		string text = All.MyDoc() + "\\WebCheck\\Backup\\" + Name + ".zip";
		bool result;
		try
		{
			GetObjectResponse @object = amazonS3Client.GetObject(getObjectRequest);
			@object.WriteResponseStreamToFile(text);
			if (@object.HttpStatusCode != HttpStatusCode.OK)
			{
				result = false;
				goto IL_00ec;
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_00ec;
		}
		using (ZipFile zipFile = ZipFile.Read(text))
		{
			try
			{
				string path = All.MyDoc() + "\\WebCheck\\Backup\\";
				zipFile.Password = Key;
				zipFile.ExtractAll(path);
			}
			catch (Exception ex3)
			{
				ProjectData.SetProjectError(ex3);
				Exception ex4 = ex3;
				result = false;
				ProjectData.ClearProjectError();
				goto IL_00ec;
			}
		}
		result = true;
		goto IL_00ec;
		IL_00ec:
		return result;
	}

	private bool DownLoadFileS3(string fl)
	{
		//IL_0014: Unknown result type (might be due to invalid IL or missing references)
		string text = "https://s3.eu-west-2.amazonaws.com/che.ck.ua/s3.txt";
		bool result;
		try
		{
			if (File.Exists(fl))
			{
				FileSystem.DeleteFile(fl);
			}
			new Network().DownloadFile(text, fl);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_0034;
		}
		result = true;
		goto IL_0034;
		IL_0034:
		return result;
	}

	private bool WriteProfile(string keyId, string secret, string profileName = "default")
	{
		bool result;
		try
		{
			CredentialProfileOptions credentialProfileOptions = new CredentialProfileOptions();
			credentialProfileOptions.AccessKey = keyId;
			credentialProfileOptions.SecretKey = secret;
			CredentialProfile profile = new CredentialProfile(profileName, credentialProfileOptions);
			new NetSDKCredentialsFile().RegisterProfile(profile);
			result = true;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private void CopyINI(string AFN)
	{
		string text = All.MyDoc() + "\\WebCheck\\Backup\\" + AFN + ".ini";
		if (File.Exists(text))
		{
			IniHGB iniHGB = new IniHGB(text);
			string section = "Backup";
			string @string = iniHGB.GetString(section, "Path");
			All.f.StringWriteFN(AFN, "Path", @string);
			@string = iniHGB.GetString(section, "TIN");
			All.f.StringWriteFN(AFN, "TIN", @string);
			@string = iniHGB.GetString(section, "On");
			All.f.StringWriteFN(AFN, "On", @string);
			@string = iniHGB.GetString(section, "Save");
			All.f.StringWriteFN(AFN, "Save", @string);
			@string = iniHGB.GetString(section, "ShowPintForm");
			All.f.StringWriteFN(AFN, "ShowPintForm", @string);
			@string = iniHGB.GetString(section, "LogOn");
			All.f.StringWriteFN(AFN, "LogOn", @string);
			@string = iniHGB.GetString(section, "FiscalMode");
			All.f.StringWriteFN(AFN, "FiscalMode", @string);
			@string = iniHGB.GetString(section, "UseACSKTSPserver");
			All.f.StringWriteFN(AFN, "UseACSKTSPserver", @string);
			@string = iniHGB.GetString(section, "Acsksettings");
			All.f.StringWriteFN(AFN, "Acsksettings", @string);
			@string = iniHGB.GetString(section, "EcoPrt");
			All.f.StringWriteFN(AFN, "EcoPrt", @string);
			@string = iniHGB.GetString(section, "ShowPintFormX");
			All.f.StringWriteFN(AFN, "ShowPintFormX", @string);
			@string = iniHGB.GetString(section, "AutomatPrintCheck");
			All.f.StringWriteFN(AFN, "AutomatPrintCheck", @string);
			@string = iniHGB.GetString(section, "Offline");
			All.f.StringWriteFN(AFN, "Offline", @string);
			@string = iniHGB.GetString(section, "AutomatOfflineOn");
			All.f.StringWriteFN(AFN, "AutomatOfflineOn", @string);
			@string = iniHGB.GetString(section, "OfflineMax");
			All.f.StringWriteFN(AFN, "OfflineMax", @string);
			@string = iniHGB.GetString(section, "OfflineMin");
			All.f.StringWriteFN(AFN, "OfflineMin", @string);
			@string = iniHGB.GetString(section, "OfflineTime");
			All.f.StringWriteFN(AFN, "OfflineTime", @string);
			@string = iniHGB.GetString(section, "ToPDF");
			All.f.StringWriteFN(AFN, "ToPDF", @string);
			@string = iniHGB.GetString(section, "ToXML");
			All.f.StringWriteFN(AFN, "ToXML", @string);
			@string = iniHGB.GetString(section, "ToTXT");
			All.f.StringWriteFN(AFN, "ToTXT", @string);
			@string = iniHGB.GetString(section, "ExportLength");
			All.f.StringWriteFN(AFN, "ExportLength", @string);
			@string = iniHGB.GetString(section, "Delay");
			All.f.StringWriteFN(AFN, "Delay", @string);
			@string = iniHGB.GetString(section, "LimitCertificate");
			All.f.StringWriteFN(AFN, "LimitCertificate", @string);
			@string = iniHGB.GetString(section, "Multiplayer");
			All.f.StringWriteFN(AFN, "Multiplayer", @string);
			@string = iniHGB.GetString(section, "AllowableCash");
			All.f.StringWriteFN(AFN, "AllowableCash", @string);
			@string = iniHGB.GetString(section, "Showacquiring");
			All.f.StringWriteFN(AFN, "Showacquiring", @string);
			@string = iniHGB.GetString(section, "MonhtLast");
			All.f.StringWriteFN(AFN, "MonhtLast", @string);
			@string = iniHGB.GetString(section, "DelTempCheck");
			All.f.StringWriteFN(AFN, "DelTempCheck", @string);
			@string = iniHGB.GetString(section, "ShowInTaskbar");
			All.f.StringWriteFN(AFN, "ShowInTaskbar", @string);
			@string = iniHGB.GetString(section, "IndicatorVisible");
			All.f.StringWriteFN(AFN, "IndicatorVisible", @string);
			@string = iniHGB.GetString(section, "IndicatorY");
			All.f.StringWriteFN(AFN, "IndicatorY", @string);
			@string = iniHGB.GetString(section, "IndicatorStepY");
			All.f.StringWriteFN(AFN, "IndicatorStepY", @string);
			@string = iniHGB.GetString(section, "PrinterWidth");
			All.f.StringWriteFN(AFN, "PrinterWidth", @string);
		}
	}

	private bool innKeyTrue()
	{
		if (Operators.CompareString(All.SF.Cert(KeyOpT.Text, PasOpT.Text).ReturnTIN, TinT.Text, false) == 0)
		{
			return true;
		}
		return false;
	}

	private void KeyB_Click(object sender, EventArgs e)
	{
		string text = PathKey();
		if (Operators.CompareString(text, "", false) == 0)
		{
			return;
		}
		KeyOpT.Text = text;
		string text2 = KeyTip(text);
		if (Operators.CompareString(text2, "zs2", false) != 0)
		{
			if (Operators.CompareString(text2, "jks", false) == 0)
			{
				All.A.AcskSettingsTemp = 4;
				Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
				if (!NewBase)
				{
					All.A.AcskSettings = All.A.AcskSettingsTemp;
					All.f.StringWriteFN(All.A.FN, "Acsksettings", All.A.AcskSettings.ToString());
				}
			}
		}
		else
		{
			All.A.AcskSettingsTemp = 2;
			Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
			if (!NewBase)
			{
				All.A.AcskSettings = All.A.AcskSettingsTemp;
				All.f.StringWriteFN(All.A.FN, "Acsksettings", All.A.AcskSettings.ToString());
			}
		}
		if (!NewBase)
		{
			if (new UpdateInfa().UPDATE("OPERATORS", "KEYPATH", "1", text).errCode == 0)
			{
				DelIni();
			}
		}
		else
		{
			((Control)PasOpT).Focus();
		}
	}

	private void DelIni()
	{
		string text = All.MyDoc() + "\\WebCheck\\Temp\\" + FnT.Text + "\\dat.ini";
		if (File.Exists(text))
		{
			FileSystem.DeleteFile(text);
		}
	}

	private string PathKey()
	{
		//IL_0000: Unknown result type (might be due to invalid IL or missing references)
		//IL_0006: Expected O, but got Unknown
		//IL_0012: Unknown result type (might be due to invalid IL or missing references)
		//IL_0018: Invalid comparison between Unknown and I4
		OpenFileDialog val = new OpenFileDialog();
		((FileDialog)val).Filter = "Key Files|*.dat;*.pfx;*.zs2;*.pk8;*.jks|All Files|*.*";
		if ((int)((CommonDialog)val).ShowDialog() == 1)
		{
			return ((FileDialog)val).FileName;
		}
		return "";
	}

	private string PathDB()
	{
		//IL_0000: Unknown result type (might be due to invalid IL or missing references)
		//IL_0006: Expected O, but got Unknown
		//IL_0012: Unknown result type (might be due to invalid IL or missing references)
		//IL_0018: Invalid comparison between Unknown and I4
		OpenFileDialog val = new OpenFileDialog();
		((FileDialog)val).Filter = "SQLite (*.db)|*.db|All Files|*.*";
		if ((int)((CommonDialog)val).ShowDialog() == 1)
		{
			return ((FileDialog)val).FileName;
		}
		return "";
	}

	private void FnT_TextChanged(object sender, EventArgs e)
	{
		if (FnT.Text.Length > 10)
		{
			FnT.Text = Strings.Mid(FnT.Text, 1, 10);
		}
		if (FnT.Text.Length == 10)
		{
			((Control)InnT).Focus();
		}
	}

	private void CheckBoxTest_CheckedChanged(object sender, EventArgs e)
	{
		((Control)GroupBox1).Enabled = !CheckBoxTest.Checked;
		((Control)GroupBox2).Enabled = !CheckBoxTest.Checked;
		if (CheckBoxTest.Checked)
		{
			TinT.Text = "34554362";
			FnT.Text = "7000000512";
			InnT.Text = "34554362";
			NorgT.Text = "Тестовий платник 3";
			NtorgT.Text = "Магазин Вебчек";
			AtorgT.Text = "м.Київ, вул. Радищева 3";
			FioOpT.Text = "Сідороренко Василь Леонідович";
			InnOpT.Text = "1111111111";
			KeyOpT.Text = "C:\\ProgramData\\WebCheck\\Keys\\Key-6.dat";
			PasOpT.Text = "tect3";
			((Control)ImportDat).Enabled = false;
			((Control)SelSwrver).Enabled = false;
			All.A.AcskSettingsTemp = 0;
			Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
			((Control)CheckBoxManual).Enabled = false;
			CheckBoxManual.Checked = false;
		}
		else
		{
			TinT.Text = "";
			FnT.Text = "";
			InnT.Text = "";
			NorgT.Text = "";
			NtorgT.Text = "";
			AtorgT.Text = "";
			FioOpT.Text = "";
			InnOpT.Text = "";
			KeyOpT.Text = "";
			PasOpT.Text = "";
			((Control)ImportDat).Enabled = true;
			((Control)SelSwrver).Enabled = true;
			All.A.AcskSettingsTemp = 0;
			Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
			((Control)CheckBoxManual).Enabled = true;
		}
	}

	private void StG_CheckedChanged(object sender, EventArgs e)
	{
	}

	private void StD_CheckedChanged(object sender, EventArgs e)
	{
	}

	private void SelSwrver_Click(object sender, EventArgs e)
	{
		//IL_000c: Unknown result type (might be due to invalid IL or missing references)
		FormServerSelection formServerSelection = new FormServerSelection(NewBase);
		((Form)formServerSelection).ShowDialog();
		((Component)(object)formServerSelection).Dispose();
		Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
	}

	private string KeyTip(string FilePath)
	{
		FilePath = FilePath.Trim();
		string text = "";
		checked
		{
			try
			{
				text = Conversions.ToString(FilePath[FilePath.Trim().Length - 3]);
				text += Conversions.ToString(FilePath[FilePath.Trim().Length - 2]);
				text += Conversions.ToString(FilePath[FilePath.Trim().Length - 1]);
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				text = "";
				ProjectData.ClearProjectError();
			}
			return text.ToLower();
		}
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		((Form)this).Close();
	}

	private void InfaTaxPay_Click(object sender, EventArgs e)
	{
		//IL_0006: Unknown result type (might be due to invalid IL or missing references)
		FormTaxPayInfo formTaxPayInfo = new FormTaxPayInfo();
		((Form)formTaxPayInfo).ShowDialog();
		((Component)(object)formTaxPayInfo).Dispose();
	}

	private void FIO_Click(object sender, EventArgs e)
	{
		//IL_0020: Unknown result type (might be due to invalid IL or missing references)
		FormEditor formEditor = new FormEditor("ПІБ оператора", FioOpT.Text, "OPERATORS", "OPERATORNAME");
		((Form)formEditor).ShowDialog();
		((Component)(object)formEditor).Dispose();
		Zupolnit();
	}

	private void INN_Click(object sender, EventArgs e)
	{
		//IL_0020: Unknown result type (might be due to invalid IL or missing references)
		FormEditor formEditor = new FormEditor("ІНН оператора", InnOpT.Text, "OPERATORS", "INN");
		((Form)formEditor).ShowDialog();
		((Component)(object)formEditor).Dispose();
		Zupolnit();
	}

	private void Pas_Click(object sender, EventArgs e)
	{
		//IL_001a: Unknown result type (might be due to invalid IL or missing references)
		FormEditor formEditor = new FormEditor("Пароль ключа ЕЦП", "*********", "OPERATORS", "KEYPASS");
		((Form)formEditor).ShowDialog();
		((Component)(object)formEditor).Dispose();
		Zupolnit();
	}

	private void Adr_Click(object sender, EventArgs e)
	{
		//IL_0020: Unknown result type (might be due to invalid IL or missing references)
		FormEditor formEditor = new FormEditor("Адреса торгової точки", AtorgT.Text, "TAXOBJECTS", "POINTADDR");
		((Form)formEditor).ShowDialog();
		((Component)(object)formEditor).Dispose();
		Zupolnit();
	}

	private void NamT_Click(object sender, EventArgs e)
	{
		//IL_0020: Unknown result type (might be due to invalid IL or missing references)
		FormEditor formEditor = new FormEditor("Назва торгової точки", NtorgT.Text, "TAXOBJECTS", "POINTNAME");
		((Form)formEditor).ShowDialog();
		((Component)(object)formEditor).Dispose();
		Zupolnit();
	}

	private void NamO_Click(object sender, EventArgs e)
	{
		//IL_0020: Unknown result type (might be due to invalid IL or missing references)
		FormEditor formEditor = new FormEditor("Назва організації", NorgT.Text, "TAXOBJECTS", "ORGNAME");
		((Form)formEditor).ShowDialog();
		((Component)(object)formEditor).Dispose();
		Zupolnit();
	}

	private void IPN_Click(object sender, EventArgs e)
	{
		//IL_0020: Unknown result type (might be due to invalid IL or missing references)
		FormEditor formEditor = new FormEditor("ІПН платника ПДВ", InnT.Text, "TAXOBJECTS", "INN");
		((Form)formEditor).ShowDialog();
		((Component)(object)formEditor).Dispose();
		Zupolnit();
	}

	private void FNN_Click(object sender, EventArgs e)
	{
		ImportDat.PerformClick();
	}

	private void EDP_Click(object sender, EventArgs e)
	{
		ImportDat.PerformClick();
	}

	private void TestPro_Click(object sender, EventArgs e)
	{
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		//IL_0018: Unknown result type (might be due to invalid IL or missing references)
		if (!All.A.Status)
		{
			Interaction.MsgBox((object)"Увага! Необхідно підключення!", (MsgBoxStyle)48, (object)"Перевірка налаштувань");
			return;
		}
		All.SF.SignatureStart();
		int acskSettings = All.A.AcskSettings;
		All.A.AcskSettings = All.A.AcskSettingsTemp;
		All.SF.SetServer();
		FormTest formTest = new FormTest(InnOpT.Text.Trim());
		((Form)formTest).ShowDialog();
		((Component)(object)formTest).Dispose();
		All.A.AcskSettings = acskSettings;
		All.SF.SetServer();
	}

	private void FormNewPro_Closing(object sender, CancelEventArgs e)
	{
		if (Operators.CompareString(All.A.FiscalMode, All.URLfact, false) == 0 && !NewBase && All.A.FN.Length > 9 && !File.Exists(Strings.Replace(All.A.FileN, All.A.FN, All.A.FN + "_TS", 1, -1, (CompareMethod)0)))
		{
			string fnS = All.A.FN + "_TS";
			CreateTables(fnS);
			CreateRow(fnS, ParOld);
		}
	}

	private void ImportDat_Click(object sender, EventArgs e)
	{
		//IL_0056: Unknown result type (might be due to invalid IL or missing references)
		//IL_0189: Unknown result type (might be due to invalid IL or missing references)
		All.SF.SignatureStart();
		if ((Operators.CompareString(KeyOpT.Text.Trim(), "", false) == 0) | (Operators.CompareString(PasOpT.Text.Trim(), "", false) == 0))
		{
			Interaction.MsgBox((object)"Обов'язкові поля для завантаження даних з кабінету податкової:\r\n- Ключ ЕЦП\r\n- Пароль до ключа ЕЦП\r\n- АЦСК", (MsgBoxStyle)48, (object)"Завантаження даних");
			return;
		}
		string text = All.MyDoc() + "\\WebCheck\\Temp\\objects.txt";
		if (File.Exists(text))
		{
			FileSystem.DeleteFile(text);
		}
		if (File.Exists(text + ".p7s"))
		{
			FileSystem.DeleteFile(text + ".p7s");
		}
		if (!FileForSend(text))
		{
			return;
		}
		int acskSettings = All.A.AcskSettings;
		All.A.AcskSettings = All.A.AcskSettingsTemp;
		All.SF.SetServer();
		int retriesPrt = All.RetriesPrt;
		All.RetriesPrt = 3;
		All.SF.ErrorShow(ShowWindows: true);
		if (All.SF.SignatureFile(KeyOpT.Text.Trim(), PasOpT.Text.Trim(), text).errCode > 0)
		{
			return;
		}
		All.SF.ErrorShow(ShowWindows: false);
		All.RetriesPrt = retriesPrt;
		All.A.AcskSettings = acskSettings;
		All.SF.SetServer();
		string text2 = SendFile(text + ".p7s");
		if (Operators.CompareString(text2.Trim(), "", false) != 0)
		{
			All.LgAll.SaveTextToLogAll("NEW PRRO JSON", text2);
			FormImport formImport = new FormImport(text2);
			((Form)formImport).ShowDialog();
			((Component)(object)formImport).Dispose();
			if (All.InfaImport.NumFiscal.Length == 10)
			{
				FnT.Text = All.InfaImport.NumFiscal;
				TinT.Text = All.InfaImport.TIN;
				InnT.Text = All.InfaImport.IPN;
				NorgT.Text = All.InfaImport.OrgName;
				NtorgT.Text = All.InfaImport.Name;
				AtorgT.Text = All.InfaImport.Address;
			}
			((Control)FioOpT).Focus();
		}
	}

	private bool FileForSend(string fileN)
	{
		bool result;
		try
		{
			StreamWriter streamWriter = new StreamWriter(fileN);
			streamWriter.Write("{\"Command\":\"Objects\"}");
			Application.DoEvents();
			streamWriter.Flush();
			streamWriter.Close();
			result = true;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private string SendFile(string FillePath)
	{
		string address = "http://fs.tax.gov.ua:8609/fs/cmd";
		string result;
		try
		{
			using WebClient webClient = new WebClient();
			webClient.Headers.Add("Content-Type", "application/octet-stream");
			Array array = File.ReadAllBytes(FillePath);
			Array array2 = webClient.UploadData(address, "POST", (byte[])array);
			string @string = Encoding.UTF8.GetString((byte[])array2);
			if (Operators.CompareString(@string.Trim(), "", false) == 0)
			{
				ShowInfaError("Виникла помилка завантаження даних");
			}
			result = @string;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ShowInfaError(ex2.Message);
			result = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private void ShowInfaError(string TextError)
	{
		//IL_00ba: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Unknown result type (might be due to invalid IL or missing references)
		TypErrStrCert typErrStrCert = All.SF.Cert(KeyOpT.Text, PasOpT.Text);
		if (typErrStrCert.errCode > 0)
		{
			Interaction.MsgBox((object)("Помилка завантаження даних з сервера податкової:\r\n" + TextError), (MsgBoxStyle)48, (object)"Помилка завантаження даних!");
			return;
		}
		Interaction.MsgBox((object)(TextError + " \r\nІнформація про сертифікат ключа: \r\n- Власник: " + typErrStrCert.ReturnSUBJCN + " \r\n- Дата початку дії: " + typErrStrCert.ReturnStart + " \r\n- Дата закінчення: " + typErrStrCert.ReturnEnd + " \r\n- TIN: " + typErrStrCert.ReturnTIN + " \r\n- Номер сертифікату: \r\n" + typErrStrCert.ReturnSerial + " \r\nне зареєстровано в податковій. \r\nПодайте форму 5-ПРРО в кабінети платника податків"), (MsgBoxStyle)48, (object)"Помилка завантаження даних!");
	}

	private void CheckBoxManual_CheckedChanged(object sender, EventArgs e)
	{
		if (CheckBoxManual.Checked)
		{
			((TextBoxBase)TinT).ReadOnly = false;
			((TextBoxBase)FnT).ReadOnly = false;
			return;
		}
		TinT.Text = "";
		((TextBoxBase)TinT).ReadOnly = true;
		FnT.Text = "";
		((TextBoxBase)FnT).ReadOnly = true;
	}
}
