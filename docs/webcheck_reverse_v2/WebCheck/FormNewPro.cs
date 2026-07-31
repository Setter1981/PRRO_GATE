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
			EventHandler value2 = FnT_TextChanged;
			TextBox fnT = _FnT;
			if (fnT != null)
			{
				fnT.TextChanged -= value2;
			}
			_FnT = value;
			fnT = _FnT;
			if (fnT != null)
			{
				fnT.TextChanged += value2;
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
			EventHandler value2 = KeyB_Click;
			Button keyB = _KeyB;
			if (keyB != null)
			{
				keyB.Click -= value2;
			}
			_KeyB = value;
			keyB = _KeyB;
			if (keyB != null)
			{
				keyB.Click += value2;
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
			EventHandler value2 = OkB_Click;
			Button okB = _OkB;
			if (okB != null)
			{
				okB.Click -= value2;
			}
			_OkB = value;
			okB = _OkB;
			if (okB != null)
			{
				okB.Click += value2;
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
			EventHandler value2 = CheckBoxTest_CheckedChanged;
			CheckBox checkBoxTest = _CheckBoxTest;
			if (checkBoxTest != null)
			{
				checkBoxTest.CheckedChanged -= value2;
			}
			_CheckBoxTest = value;
			checkBoxTest = _CheckBoxTest;
			if (checkBoxTest != null)
			{
				checkBoxTest.CheckedChanged += value2;
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
			EventHandler value2 = SelSwrver_Click;
			Button selSwrver = _SelSwrver;
			if (selSwrver != null)
			{
				selSwrver.Click -= value2;
			}
			_SelSwrver = value;
			selSwrver = _SelSwrver;
			if (selSwrver != null)
			{
				selSwrver.Click += value2;
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
			EventHandler value2 = NoB_Click;
			Button noB = _NoB;
			if (noB != null)
			{
				noB.Click -= value2;
			}
			_NoB = value;
			noB = _NoB;
			if (noB != null)
			{
				noB.Click += value2;
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
			EventHandler value2 = Adr_Click;
			Button adr = _Adr;
			if (adr != null)
			{
				adr.Click -= value2;
			}
			_Adr = value;
			adr = _Adr;
			if (adr != null)
			{
				adr.Click += value2;
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
			EventHandler value2 = NamT_Click;
			Button namT = _NamT;
			if (namT != null)
			{
				namT.Click -= value2;
			}
			_NamT = value;
			namT = _NamT;
			if (namT != null)
			{
				namT.Click += value2;
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
			EventHandler value2 = NamO_Click;
			Button namO = _NamO;
			if (namO != null)
			{
				namO.Click -= value2;
			}
			_NamO = value;
			namO = _NamO;
			if (namO != null)
			{
				namO.Click += value2;
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
			EventHandler value2 = Pas_Click;
			Button pas = _Pas;
			if (pas != null)
			{
				pas.Click -= value2;
			}
			_Pas = value;
			pas = _Pas;
			if (pas != null)
			{
				pas.Click += value2;
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
			EventHandler value2 = INN_Click;
			Button iNN = _INN;
			if (iNN != null)
			{
				iNN.Click -= value2;
			}
			_INN = value;
			iNN = _INN;
			if (iNN != null)
			{
				iNN.Click += value2;
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
			EventHandler value2 = FIO_Click;
			Button fIO = _FIO;
			if (fIO != null)
			{
				fIO.Click -= value2;
			}
			_FIO = value;
			fIO = _FIO;
			if (fIO != null)
			{
				fIO.Click += value2;
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
			EventHandler value2 = InfaTaxPay_Click;
			Button infaTaxPay = _InfaTaxPay;
			if (infaTaxPay != null)
			{
				infaTaxPay.Click -= value2;
			}
			_InfaTaxPay = value;
			infaTaxPay = _InfaTaxPay;
			if (infaTaxPay != null)
			{
				infaTaxPay.Click += value2;
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
			EventHandler value2 = IPN_Click;
			Button iPN = _IPN;
			if (iPN != null)
			{
				iPN.Click -= value2;
			}
			_IPN = value;
			iPN = _IPN;
			if (iPN != null)
			{
				iPN.Click += value2;
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
			EventHandler value2 = EDP_Click;
			Button eDP = _EDP;
			if (eDP != null)
			{
				eDP.Click -= value2;
			}
			_EDP = value;
			eDP = _EDP;
			if (eDP != null)
			{
				eDP.Click += value2;
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
			EventHandler value2 = TestPro_Click;
			Button testPro = _TestPro;
			if (testPro != null)
			{
				testPro.Click -= value2;
			}
			_TestPro = value;
			testPro = _TestPro;
			if (testPro != null)
			{
				testPro.Click += value2;
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
			EventHandler value2 = ImportDat_Click;
			Button importDat = _ImportDat;
			if (importDat != null)
			{
				importDat.Click -= value2;
			}
			_ImportDat = value;
			importDat = _ImportDat;
			if (importDat != null)
			{
				importDat.Click += value2;
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
			EventHandler value2 = FNN_Click;
			Button fNN = _FNN;
			if (fNN != null)
			{
				fNN.Click -= value2;
			}
			_FNN = value;
			fNN = _FNN;
			if (fNN != null)
			{
				fNN.Click += value2;
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
			EventHandler value2 = CheckBoxManual_CheckedChanged;
			CheckBox checkBoxManual = _CheckBoxManual;
			if (checkBoxManual != null)
			{
				checkBoxManual.CheckedChanged -= value2;
			}
			_CheckBoxManual = value;
			checkBoxManual = _CheckBoxManual;
			if (checkBoxManual != null)
			{
				checkBoxManual.CheckedChanged += value2;
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
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormNewPro));
		this.Label1 = new System.Windows.Forms.Label();
		this.GroupBox1 = new System.Windows.Forms.GroupBox();
		this.FNN = new System.Windows.Forms.Button();
		this.IPN = new System.Windows.Forms.Button();
		this.EDP = new System.Windows.Forms.Button();
		this.Adr = new System.Windows.Forms.Button();
		this.NamT = new System.Windows.Forms.Button();
		this.NamO = new System.Windows.Forms.Button();
		this.Label4 = new System.Windows.Forms.Label();
		this.Label7 = new System.Windows.Forms.Label();
		this.AtorgT = new System.Windows.Forms.TextBox();
		this.Label5 = new System.Windows.Forms.Label();
		this.Label6 = new System.Windows.Forms.Label();
		this.NtorgT = new System.Windows.Forms.TextBox();
		this.NorgT = new System.Windows.Forms.TextBox();
		this.InnT = new System.Windows.Forms.TextBox();
		this.Label3 = new System.Windows.Forms.Label();
		this.FnT = new System.Windows.Forms.TextBox();
		this.Label2 = new System.Windows.Forms.Label();
		this.TinT = new System.Windows.Forms.TextBox();
		this.GroupBox2 = new System.Windows.Forms.GroupBox();
		this.Pas = new System.Windows.Forms.Button();
		this.INN = new System.Windows.Forms.Button();
		this.FIO = new System.Windows.Forms.Button();
		this.KeyB = new System.Windows.Forms.Button();
		this.Label11 = new System.Windows.Forms.Label();
		this.Label10 = new System.Windows.Forms.Label();
		this.Label9 = new System.Windows.Forms.Label();
		this.PasOpT = new System.Windows.Forms.TextBox();
		this.KeyOpT = new System.Windows.Forms.TextBox();
		this.InnOpT = new System.Windows.Forms.TextBox();
		this.FioOpT = new System.Windows.Forms.TextBox();
		this.Label8 = new System.Windows.Forms.Label();
		this.OkB = new System.Windows.Forms.Button();
		this.CheckBoxTest = new System.Windows.Forms.CheckBox();
		this.SelSwrver = new System.Windows.Forms.Button();
		this.GroupBox3 = new System.Windows.Forms.GroupBox();
		this.CheckBoxManual = new System.Windows.Forms.CheckBox();
		this.ImportDat = new System.Windows.Forms.Button();
		this.TestPro = new System.Windows.Forms.Button();
		this.Server = new System.Windows.Forms.TextBox();
		this.Label21 = new System.Windows.Forms.Label();
		this.NoB = new System.Windows.Forms.Button();
		this.Logo = new System.Windows.Forms.PictureBox();
		this.InfaTaxPay = new System.Windows.Forms.Button();
		this.PrintDialog1 = new System.Windows.Forms.PrintDialog();
		this.GroupBox1.SuspendLayout();
		this.GroupBox2.SuspendLayout();
		this.GroupBox3.SuspendLayout();
		((System.ComponentModel.ISupportInitialize)this.Logo).BeginInit();
		base.SuspendLayout();
		this.Label1.AutoSize = true;
		this.Label1.Font = new System.Drawing.Font("Microsoft Sans Serif", 16.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label1.Location = new System.Drawing.Point(18, 9);
		this.Label1.Name = "Label1";
		this.Label1.Size = new System.Drawing.Size(476, 32);
		this.Label1.TabIndex = 0;
		this.Label1.Text = "Майстер заповнення нового ПРРО";
		this.GroupBox1.Controls.Add(this.FNN);
		this.GroupBox1.Controls.Add(this.IPN);
		this.GroupBox1.Controls.Add(this.EDP);
		this.GroupBox1.Controls.Add(this.Adr);
		this.GroupBox1.Controls.Add(this.NamT);
		this.GroupBox1.Controls.Add(this.NamO);
		this.GroupBox1.Controls.Add(this.Label4);
		this.GroupBox1.Controls.Add(this.Label7);
		this.GroupBox1.Controls.Add(this.AtorgT);
		this.GroupBox1.Controls.Add(this.Label5);
		this.GroupBox1.Controls.Add(this.Label6);
		this.GroupBox1.Controls.Add(this.NtorgT);
		this.GroupBox1.Controls.Add(this.NorgT);
		this.GroupBox1.Controls.Add(this.InnT);
		this.GroupBox1.Controls.Add(this.Label3);
		this.GroupBox1.Controls.Add(this.FnT);
		this.GroupBox1.Controls.Add(this.Label2);
		this.GroupBox1.Controls.Add(this.TinT);
		this.GroupBox1.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.GroupBox1.Location = new System.Drawing.Point(12, 55);
		this.GroupBox1.Name = "GroupBox1";
		this.GroupBox1.Size = new System.Drawing.Size(660, 271);
		this.GroupBox1.TabIndex = 0;
		this.GroupBox1.TabStop = false;
		this.GroupBox1.Text = "Організація";
		this.FNN.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.FNN.Location = new System.Drawing.Point(588, 65);
		this.FNN.Name = "FNN";
		this.FNN.Size = new System.Drawing.Size(53, 30);
		this.FNN.TabIndex = 26;
		this.FNN.Text = "...";
		this.FNN.UseVisualStyleBackColor = true;
		this.IPN.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.IPN.Location = new System.Drawing.Point(588, 107);
		this.IPN.Name = "IPN";
		this.IPN.Size = new System.Drawing.Size(53, 30);
		this.IPN.TabIndex = 25;
		this.IPN.Text = "...";
		this.IPN.UseVisualStyleBackColor = true;
		this.EDP.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.EDP.Location = new System.Drawing.Point(588, 26);
		this.EDP.Name = "EDP";
		this.EDP.Size = new System.Drawing.Size(53, 30);
		this.EDP.TabIndex = 24;
		this.EDP.Text = "...";
		this.EDP.UseVisualStyleBackColor = true;
		this.Adr.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Adr.Location = new System.Drawing.Point(588, 226);
		this.Adr.Name = "Adr";
		this.Adr.Size = new System.Drawing.Size(53, 30);
		this.Adr.TabIndex = 25;
		this.Adr.Text = "...";
		this.Adr.UseVisualStyleBackColor = true;
		this.NamT.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NamT.Location = new System.Drawing.Point(588, 190);
		this.NamT.Name = "NamT";
		this.NamT.Size = new System.Drawing.Size(53, 30);
		this.NamT.TabIndex = 24;
		this.NamT.Text = "...";
		this.NamT.UseVisualStyleBackColor = true;
		this.NamO.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NamO.Location = new System.Drawing.Point(588, 150);
		this.NamO.Name = "NamO";
		this.NamO.Size = new System.Drawing.Size(53, 30);
		this.NamO.TabIndex = 23;
		this.NamO.Text = "...";
		this.NamO.UseVisualStyleBackColor = true;
		this.Label4.AutoSize = true;
		this.Label4.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label4.Location = new System.Drawing.Point(9, 110);
		this.Label4.Name = "Label4";
		this.Label4.Size = new System.Drawing.Size(184, 25);
		this.Label4.TabIndex = 7;
		this.Label4.Text = "ІПН платника ПДВ";
		this.Label7.AutoSize = true;
		this.Label7.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label7.Location = new System.Drawing.Point(9, 230);
		this.Label7.Name = "Label7";
		this.Label7.Size = new System.Drawing.Size(237, 25);
		this.Label7.TabIndex = 11;
		this.Label7.Text = "Адреса торгової точки *";
		this.AtorgT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.AtorgT.Location = new System.Drawing.Point(270, 227);
		this.AtorgT.Name = "AtorgT";
		this.AtorgT.Size = new System.Drawing.Size(309, 30);
		this.AtorgT.TabIndex = 10;
		this.AtorgT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Label5.AutoSize = true;
		this.Label5.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label5.Location = new System.Drawing.Point(9, 150);
		this.Label5.Name = "Label5";
		this.Label5.Size = new System.Drawing.Size(182, 25);
		this.Label5.TabIndex = 8;
		this.Label5.Text = "Назва організації *";
		this.Label6.AutoSize = true;
		this.Label6.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label6.Location = new System.Drawing.Point(9, 190);
		this.Label6.Name = "Label6";
		this.Label6.Size = new System.Drawing.Size(224, 25);
		this.Label6.TabIndex = 9;
		this.Label6.Text = "Назва торгової точки *";
		this.NtorgT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NtorgT.Location = new System.Drawing.Point(270, 187);
		this.NtorgT.Name = "NtorgT";
		this.NtorgT.Size = new System.Drawing.Size(309, 30);
		this.NtorgT.TabIndex = 6;
		this.NtorgT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.NorgT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NorgT.Location = new System.Drawing.Point(270, 147);
		this.NorgT.Name = "NorgT";
		this.NorgT.Size = new System.Drawing.Size(309, 30);
		this.NorgT.TabIndex = 5;
		this.NorgT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.InnT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.InnT.Location = new System.Drawing.Point(270, 107);
		this.InnT.Name = "InnT";
		this.InnT.Size = new System.Drawing.Size(309, 30);
		this.InnT.TabIndex = 4;
		this.InnT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Label3.AutoSize = true;
		this.Label3.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label3.Location = new System.Drawing.Point(9, 70);
		this.Label3.Name = "Label3";
		this.Label3.Size = new System.Drawing.Size(199, 25);
		this.Label3.TabIndex = 3;
		this.Label3.Text = "Фіскальний номер *";
		this.FnT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.FnT.Location = new System.Drawing.Point(270, 67);
		this.FnT.Name = "FnT";
		this.FnT.ReadOnly = true;
		this.FnT.Size = new System.Drawing.Size(309, 30);
		this.FnT.TabIndex = 2;
		this.FnT.TabStop = false;
		this.FnT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Label2.AutoSize = true;
		this.Label2.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label2.Location = new System.Drawing.Point(9, 29);
		this.Label2.Name = "Label2";
		this.Label2.Size = new System.Drawing.Size(108, 25);
		this.Label2.TabIndex = 1;
		this.Label2.Text = "ЕДРПОУ *";
		this.TinT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TinT.Location = new System.Drawing.Point(270, 27);
		this.TinT.Name = "TinT";
		this.TinT.ReadOnly = true;
		this.TinT.Size = new System.Drawing.Size(309, 30);
		this.TinT.TabIndex = 0;
		this.TinT.TabStop = false;
		this.TinT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.GroupBox2.Controls.Add(this.Pas);
		this.GroupBox2.Controls.Add(this.INN);
		this.GroupBox2.Controls.Add(this.FIO);
		this.GroupBox2.Controls.Add(this.KeyB);
		this.GroupBox2.Controls.Add(this.Label11);
		this.GroupBox2.Controls.Add(this.Label10);
		this.GroupBox2.Controls.Add(this.Label9);
		this.GroupBox2.Controls.Add(this.PasOpT);
		this.GroupBox2.Controls.Add(this.KeyOpT);
		this.GroupBox2.Controls.Add(this.InnOpT);
		this.GroupBox2.Controls.Add(this.FioOpT);
		this.GroupBox2.Controls.Add(this.Label8);
		this.GroupBox2.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.GroupBox2.Location = new System.Drawing.Point(12, 332);
		this.GroupBox2.Name = "GroupBox2";
		this.GroupBox2.Size = new System.Drawing.Size(660, 201);
		this.GroupBox2.TabIndex = 1;
		this.GroupBox2.TabStop = false;
		this.GroupBox2.Text = "Оператор";
		this.Pas.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Pas.Location = new System.Drawing.Point(588, 153);
		this.Pas.Name = "Pas";
		this.Pas.Size = new System.Drawing.Size(53, 30);
		this.Pas.TabIndex = 24;
		this.Pas.Text = "...";
		this.Pas.UseVisualStyleBackColor = true;
		this.INN.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.INN.Location = new System.Drawing.Point(588, 73);
		this.INN.Name = "INN";
		this.INN.Size = new System.Drawing.Size(53, 30);
		this.INN.TabIndex = 23;
		this.INN.Text = "...";
		this.INN.UseVisualStyleBackColor = true;
		this.FIO.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.FIO.Location = new System.Drawing.Point(588, 32);
		this.FIO.Name = "FIO";
		this.FIO.Size = new System.Drawing.Size(53, 30);
		this.FIO.TabIndex = 22;
		this.FIO.Text = "...";
		this.FIO.UseVisualStyleBackColor = true;
		this.KeyB.Location = new System.Drawing.Point(588, 113);
		this.KeyB.Name = "KeyB";
		this.KeyB.Size = new System.Drawing.Size(53, 30);
		this.KeyB.TabIndex = 0;
		this.KeyB.Text = "...";
		this.KeyB.UseVisualStyleBackColor = true;
		this.Label11.AutoSize = true;
		this.Label11.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label11.Location = new System.Drawing.Point(9, 156);
		this.Label11.Name = "Label11";
		this.Label11.Size = new System.Drawing.Size(202, 25);
		this.Label11.TabIndex = 18;
		this.Label11.Text = "Пароль ключа ЕЦП *";
		this.Label10.AutoSize = true;
		this.Label10.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label10.Location = new System.Drawing.Point(9, 116);
		this.Label10.Name = "Label10";
		this.Label10.Size = new System.Drawing.Size(121, 25);
		this.Label10.TabIndex = 17;
		this.Label10.Text = "Ключ ЕЦП *";
		this.Label9.AutoSize = true;
		this.Label9.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label9.Location = new System.Drawing.Point(9, 76);
		this.Label9.Name = "Label9";
		this.Label9.Size = new System.Drawing.Size(159, 25);
		this.Label9.TabIndex = 8;
		this.Label9.Text = "ІНН оператора *";
		this.PasOpT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.PasOpT.Location = new System.Drawing.Point(270, 153);
		this.PasOpT.Name = "PasOpT";
		this.PasOpT.PasswordChar = '*';
		this.PasOpT.Size = new System.Drawing.Size(309, 30);
		this.PasOpT.TabIndex = 16;
		this.PasOpT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.KeyOpT.Enabled = false;
		this.KeyOpT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.KeyOpT.Location = new System.Drawing.Point(270, 113);
		this.KeyOpT.Name = "KeyOpT";
		this.KeyOpT.Size = new System.Drawing.Size(309, 30);
		this.KeyOpT.TabIndex = 15;
		this.KeyOpT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.InnOpT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.InnOpT.Location = new System.Drawing.Point(270, 73);
		this.InnOpT.Name = "InnOpT";
		this.InnOpT.Size = new System.Drawing.Size(309, 30);
		this.InnOpT.TabIndex = 14;
		this.InnOpT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.FioOpT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.FioOpT.Location = new System.Drawing.Point(270, 32);
		this.FioOpT.Name = "FioOpT";
		this.FioOpT.Size = new System.Drawing.Size(309, 30);
		this.FioOpT.TabIndex = 13;
		this.FioOpT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Label8.AutoSize = true;
		this.Label8.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label8.Location = new System.Drawing.Point(9, 35);
		this.Label8.Name = "Label8";
		this.Label8.Size = new System.Drawing.Size(159, 25);
		this.Label8.TabIndex = 2;
		this.Label8.Text = "ПІБ оператора *";
		this.OkB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OkB.Location = new System.Drawing.Point(912, 493);
		this.OkB.Name = "OkB";
		this.OkB.Size = new System.Drawing.Size(132, 40);
		this.OkB.TabIndex = 4;
		this.OkB.Text = "Створити";
		this.OkB.UseVisualStyleBackColor = true;
		this.CheckBoxTest.AutoSize = true;
		this.CheckBoxTest.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.CheckBoxTest.Location = new System.Drawing.Point(22, 249);
		this.CheckBoxTest.Name = "CheckBoxTest";
		this.CheckBoxTest.Size = new System.Drawing.Size(299, 24);
		this.CheckBoxTest.TabIndex = 18;
		this.CheckBoxTest.Text = "Заповнити даними для тестів...";
		this.CheckBoxTest.UseVisualStyleBackColor = true;
		this.SelSwrver.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.SelSwrver.Location = new System.Drawing.Point(288, 40);
		this.SelSwrver.Name = "SelSwrver";
		this.SelSwrver.Size = new System.Drawing.Size(53, 30);
		this.SelSwrver.TabIndex = 19;
		this.SelSwrver.Text = "...";
		this.SelSwrver.UseVisualStyleBackColor = true;
		this.GroupBox3.Controls.Add(this.CheckBoxManual);
		this.GroupBox3.Controls.Add(this.ImportDat);
		this.GroupBox3.Controls.Add(this.TestPro);
		this.GroupBox3.Controls.Add(this.Server);
		this.GroupBox3.Controls.Add(this.SelSwrver);
		this.GroupBox3.Controls.Add(this.Label21);
		this.GroupBox3.Controls.Add(this.CheckBoxTest);
		this.GroupBox3.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.GroupBox3.Location = new System.Drawing.Point(690, 126);
		this.GroupBox3.Name = "GroupBox3";
		this.GroupBox3.Size = new System.Drawing.Size(354, 349);
		this.GroupBox3.TabIndex = 19;
		this.GroupBox3.TabStop = false;
		this.GroupBox3.Text = "Додатковe налаштування";
		this.CheckBoxManual.AutoSize = true;
		this.CheckBoxManual.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.CheckBoxManual.Location = new System.Drawing.Point(22, 288);
		this.CheckBoxManual.Name = "CheckBoxManual";
		this.CheckBoxManual.Size = new System.Drawing.Size(319, 44);
		this.CheckBoxManual.TabIndex = 24;
		this.CheckBoxManual.TabStop = false;
		this.CheckBoxManual.Text = "Ручне заповнення даних. Увага! \r\n(Без відновлення резервної копії)";
		this.CheckBoxManual.TextAlign = System.Drawing.ContentAlignment.MiddleCenter;
		this.CheckBoxManual.UseVisualStyleBackColor = true;
		this.ImportDat.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.ImportDat.Location = new System.Drawing.Point(11, 144);
		this.ImportDat.Name = "ImportDat";
		this.ImportDat.Size = new System.Drawing.Size(330, 78);
		this.ImportDat.TabIndex = 23;
		this.ImportDat.Text = "Завантаження даних з кабінету податкової...";
		this.ImportDat.UseVisualStyleBackColor = true;
		this.TestPro.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TestPro.Location = new System.Drawing.Point(11, 91);
		this.TestPro.Name = "TestPro";
		this.TestPro.Size = new System.Drawing.Size(330, 38);
		this.TestPro.TabIndex = 21;
		this.TestPro.Text = "Перевірка налаштувань...";
		this.TestPro.UseVisualStyleBackColor = true;
		this.Server.Enabled = false;
		this.Server.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Server.Location = new System.Drawing.Point(76, 40);
		this.Server.Name = "Server";
		this.Server.Size = new System.Drawing.Size(202, 30);
		this.Server.TabIndex = 20;
		this.Server.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Label21.AutoSize = true;
		this.Label21.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label21.Location = new System.Drawing.Point(6, 43);
		this.Label21.Name = "Label21";
		this.Label21.Size = new System.Drawing.Size(64, 25);
		this.Label21.TabIndex = 8;
		this.Label21.Text = "АЦСК";
		this.NoB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NoB.Location = new System.Drawing.Point(690, 493);
		this.NoB.Name = "NoB";
		this.NoB.Size = new System.Drawing.Size(132, 40);
		this.NoB.TabIndex = 20;
		this.NoB.Text = "Скасувати";
		this.NoB.UseVisualStyleBackColor = true;
		this.Logo.Location = new System.Drawing.Point(690, 12);
		this.Logo.Name = "Logo";
		this.Logo.Size = new System.Drawing.Size(354, 108);
		this.Logo.SizeMode = System.Windows.Forms.PictureBoxSizeMode.Zoom;
		this.Logo.TabIndex = 21;
		this.Logo.TabStop = false;
		this.InfaTaxPay.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.InfaTaxPay.Location = new System.Drawing.Point(540, 12);
		this.InfaTaxPay.Name = "InfaTaxPay";
		this.InfaTaxPay.Size = new System.Drawing.Size(132, 40);
		this.InfaTaxPay.TabIndex = 21;
		this.InfaTaxPay.Text = "Iнфо";
		this.InfaTaxPay.UseVisualStyleBackColor = true;
		this.PrintDialog1.UseEXDialog = true;
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(1063, 544);
		base.Controls.Add(this.InfaTaxPay);
		base.Controls.Add(this.NoB);
		base.Controls.Add(this.GroupBox3);
		base.Controls.Add(this.OkB);
		base.Controls.Add(this.GroupBox2);
		base.Controls.Add(this.GroupBox1);
		base.Controls.Add(this.Label1);
		base.Controls.Add(this.Logo);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormNewPro";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Новий ПРРО";
		this.GroupBox1.ResumeLayout(false);
		this.GroupBox1.PerformLayout();
		this.GroupBox2.ResumeLayout(false);
		this.GroupBox2.PerformLayout();
		this.GroupBox3.ResumeLayout(false);
		this.GroupBox3.PerformLayout();
		((System.ComponentModel.ISupportInitialize)this.Logo).EndInit();
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	public FormNewPro(string FnStr, string OperatorID, bool DemoInfa = false, bool DemoOnly = false)
	{
		base.Load += FormNewPro_Load;
		base.Closing += FormNewPro_Closing;
		ParOld = "";
		InitializeComponent();
		All.A.AcskSettingsTemp = 0;
		if (DemoOnly)
		{
			CheckBoxTest.Checked = true;
			CheckBoxTest.Enabled = false;
			Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
			InfaTaxPay.Enabled = false;
			SelSwrver.Enabled = false;
			ImportDat.Enabled = false;
		}
		else if (DemoInfa)
		{
			ImportDat.Enabled = false;
			NewBase = false;
			Text = "Data RRO ";
			Label1.Text = "Основна інформація:";
			CheckBoxTest.Visible = false;
			OkB.Visible = false;
			NoB.Text = "Закрити";
			NoB.Left = OkB.Left;
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
				int num = 1;
				FioOpT.Text = operatorsAll.get_Seller(1, num);
				KeyOpT.Text = operatorsAll.get_Seller(2, num);
				PasOpT.Text = "*********";
				InnOpT.Text = operatorsAll.get_Seller(4, num);
				Coding coding = new Coding();
				ParOld = coding.DeCod(operatorsAll.get_Seller(3, num));
			}
			if ((FnStr.Length == 10) & Versioned.IsNumeric(FnStr))
			{
				CheckBoxTest.Enabled = false;
				FnT.Enabled = false;
				FnT.Text = FnStr;
			}
			if (OperatorID.Trim().Length > 0)
			{
				CheckBoxTest.Enabled = false;
				InnOpT.Enabled = false;
				InnOpT.Text = OperatorID;
			}
		}
		else
		{
			ImportDat.Enabled = true;
			NewBase = true;
			InfaTaxPay.Enabled = false;
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
		base.AcceptButton = OkB;
		base.CancelButton = NoB;
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
			TinT.Enabled = false;
			FnT.Enabled = false;
			InnT.Enabled = false;
			NorgT.Enabled = false;
			NtorgT.Enabled = false;
			AtorgT.Enabled = false;
			FioOpT.Enabled = false;
			InnOpT.Enabled = false;
			PasOpT.Enabled = false;
			EDP.Enabled = false;
			FNN.Enabled = false;
			IPN.Enabled = true;
			NamO.Enabled = true;
			NamT.Enabled = true;
			Adr.Enabled = true;
			FIO.Enabled = true;
			INN.Enabled = true;
			KeyB.Enabled = true;
			Pas.Enabled = true;
			SelSwrver.Enabled = true;
			TestPro.Enabled = true;
			CheckBoxManual.Enabled = false;
		}
		else
		{
			TinT.Enabled = true;
			FnT.Enabled = true;
			InnT.Enabled = true;
			NorgT.Enabled = true;
			NtorgT.Enabled = true;
			AtorgT.Enabled = true;
			FioOpT.Enabled = true;
			InnOpT.Enabled = true;
			PasOpT.Enabled = true;
			EDP.Enabled = true;
			FNN.Enabled = true;
			IPN.Enabled = false;
			NamO.Enabled = false;
			NamT.Enabled = false;
			Adr.Enabled = false;
			FIO.Enabled = false;
			INN.Enabled = false;
			KeyB.Enabled = true;
			Pas.Enabled = false;
			SelSwrver.Enabled = true;
			TestPro.Enabled = false;
			CheckBoxManual.Enabled = true;
		}
	}

	public bool CreateRow(string FnS, string NewPar = "")
	{
		CreateDB createDB = new CreateDB(FnS);
		createDB.SaveTaxObjects(FnT.Text, TinT.Text, InnT.Text, All.l.TextToTextSQL(NtorgT.Text), All.l.TextToTextSQL(NorgT.Text), All.l.TextToTextSQL(AtorgT.Text));
		createDB.SaveOperators(PassS: (Operators.CompareString(NewPar.Trim(), "", TextCompare: false) == 0) ? PasOpT.Text : NewPar, FioS: All.l.TextToTextXML(FioOpT.Text), PathKS: KeyOpT.Text, InnS: InnOpT.Text);
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
		if (CheckBoxManual.Checked)
		{
			if (Operators.CompareString(TinT.Text.Trim(), "", TextCompare: false) == 0)
			{
				TinT.Focus();
				return;
			}
			if (Operators.CompareString(FnT.Text.Trim(), "", TextCompare: false) == 0)
			{
				FnT.Focus();
				return;
			}
			if (FnT.Text.Length != 10)
			{
				Interaction.MsgBox("Не вірний формат фіскального номера!", MsgBoxStyle.Exclamation, "Новий PRO");
				FnT.Focus();
				return;
			}
			if (!Versioned.IsNumeric(FnT.Text))
			{
				Interaction.MsgBox("Не вірний формат фіскального номера!", MsgBoxStyle.Exclamation, "Новий PRO");
				FnT.Focus();
				return;
			}
		}
		if ((Operators.CompareString(TinT.Text.Trim(), "", TextCompare: false) == 0) | (Operators.CompareString(FnT.Text.Trim(), "", TextCompare: false) == 0))
		{
			ImportDat.PerformClick();
		}
		if (Operators.CompareString(NorgT.Text.Trim(), "", TextCompare: false) == 0)
		{
			NorgT.Focus();
			return;
		}
		if (Operators.CompareString(NtorgT.Text.Trim(), "", TextCompare: false) == 0)
		{
			NtorgT.Focus();
			return;
		}
		if (Operators.CompareString(AtorgT.Text.Trim(), "", TextCompare: false) == 0)
		{
			AtorgT.Focus();
			return;
		}
		if (Operators.CompareString(FioOpT.Text.Trim(), "", TextCompare: false) == 0)
		{
			FioOpT.Focus();
			return;
		}
		if (Operators.CompareString(InnOpT.Text.Trim(), "", TextCompare: false) == 0)
		{
			InnOpT.Focus();
			return;
		}
		if (Operators.CompareString(KeyOpT.Text.Trim(), "", TextCompare: false) == 0)
		{
			string left = PathKey();
			if (Operators.CompareString(left, "", TextCompare: false) != 0)
			{
				KeyOpT.Text = left;
				PasOpT.Focus();
			}
			return;
		}
		if (Operators.CompareString(PasOpT.Text.Trim(), "", TextCompare: false) == 0)
		{
			PasOpT.Focus();
			return;
		}
		if (Operators.CompareString(Server.Text, "", TextCompare: false) == 0)
		{
			new FormServerSelection(NewBase).ShowDialog();
			Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
		}
		string text = All.MyDoc() + "\\WebCheck\\DB\\" + FnT.Text + ".db";
		string text2 = "";
		if (TestBackup(text))
		{
			text2 = "Резервна база встановлена і підключена!";
		}
		if (All.f.IndexFn(FnT.Text) == 0)
		{
			if (Interaction.MsgBox("Створити нову базу даних?", MsgBoxStyle.OkCancel | MsgBoxStyle.Question, "Новий PRO") == MsgBoxResult.Ok)
			{
				All.f.AddFn(FnT.Text);
				All.f.StringWriteFN(FnT.Text, "Path", text);
				All.f.StringWriteFN(FnT.Text, "TIN", TinT.Text);
				All.f.StringWriteFN(FnT.Text, "On", "1");
				All.f.StringWriteFN(FnT.Text, "Save", "0");
				All.f.StringWriteFN(FnT.Text, "ShowPintForm", "1");
				All.f.StringWriteFN(FnT.Text, "LogOn", "1");
				All.f.StringWriteFN(All.A.FN, "Acsksettings", All.A.AcskSettingsTemp.ToString());
				if (File.Exists(text) && Operators.CompareString(text2, "", TextCompare: false) == 0)
				{
					text2 = "База підключена!";
				}
				OkB.Enabled = false;
				base.Enabled = false;
				string text3 = FnT.Text.Trim();
				CreateTables(text3);
				CreateRow(text3);
				CopyINI(text3);
				text3 += "_TS";
				CreateTables(text3);
				CreateRow(text3);
				All.NewFolderFn();
				All.A.AcskSettings = All.f.IntegerGetFn(All.A.FN, "Acsksettings");
				if (Operators.CompareString(All.f.StringGetFn(All.A.FN, "Acsksettings"), "", TextCompare: false) == 0)
				{
					All.f.StringWriteFN(All.A.FN, "Acsksettings", All.A.AcskSettingsTemp.ToString());
				}
				if (Operators.CompareString(text2, "", TextCompare: false) == 0)
				{
					StartBackup(text);
					text2 = "База успішно створена!";
				}
				Interaction.MsgBox(text2, MsgBoxStyle.Information, "Новий PRO");
				Close();
			}
		}
		else
		{
			Interaction.MsgBox("Такий FN вже є!", MsgBoxStyle.Exclamation, "Новий PRO");
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
						Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(f);
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
		string address = "https://s3.eu-west-2.amazonaws.com/che.ck.ua/s3.txt";
		bool result;
		try
		{
			if (File.Exists(fl))
			{
				Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(fl);
			}
			new Network().DownloadFile(address, fl);
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
		if (Operators.CompareString(All.SF.Cert(KeyOpT.Text, PasOpT.Text).ReturnTIN, TinT.Text, TextCompare: false) == 0)
		{
			return true;
		}
		return false;
	}

	private void KeyB_Click(object sender, EventArgs e)
	{
		string text = PathKey();
		if (Operators.CompareString(text, "", TextCompare: false) == 0)
		{
			return;
		}
		KeyOpT.Text = text;
		string left = KeyTip(text);
		if (Operators.CompareString(left, "zs2", TextCompare: false) != 0)
		{
			if (Operators.CompareString(left, "jks", TextCompare: false) == 0)
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
			PasOpT.Focus();
		}
	}

	private void DelIni()
	{
		string text = All.MyDoc() + "\\WebCheck\\Temp\\" + FnT.Text + "\\dat.ini";
		if (File.Exists(text))
		{
			Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(text);
		}
	}

	private string PathKey()
	{
		OpenFileDialog openFileDialog = new OpenFileDialog();
		openFileDialog.Filter = "Key Files|*.dat;*.pfx;*.zs2;*.pk8;*.jks|All Files|*.*";
		if (openFileDialog.ShowDialog() == DialogResult.OK)
		{
			return openFileDialog.FileName;
		}
		return "";
	}

	private string PathDB()
	{
		OpenFileDialog openFileDialog = new OpenFileDialog();
		openFileDialog.Filter = "SQLite (*.db)|*.db|All Files|*.*";
		if (openFileDialog.ShowDialog() == DialogResult.OK)
		{
			return openFileDialog.FileName;
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
			InnT.Focus();
		}
	}

	private void CheckBoxTest_CheckedChanged(object sender, EventArgs e)
	{
		GroupBox1.Enabled = !CheckBoxTest.Checked;
		GroupBox2.Enabled = !CheckBoxTest.Checked;
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
			ImportDat.Enabled = false;
			SelSwrver.Enabled = false;
			All.A.AcskSettingsTemp = 0;
			Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
			CheckBoxManual.Enabled = false;
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
			ImportDat.Enabled = true;
			SelSwrver.Enabled = true;
			All.A.AcskSettingsTemp = 0;
			Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
			CheckBoxManual.Enabled = true;
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
		FormServerSelection formServerSelection = new FormServerSelection(NewBase);
		formServerSelection.ShowDialog();
		formServerSelection.Dispose();
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
		Close();
	}

	private void InfaTaxPay_Click(object sender, EventArgs e)
	{
		FormTaxPayInfo formTaxPayInfo = new FormTaxPayInfo();
		formTaxPayInfo.ShowDialog();
		formTaxPayInfo.Dispose();
	}

	private void FIO_Click(object sender, EventArgs e)
	{
		FormEditor formEditor = new FormEditor("ПІБ оператора", FioOpT.Text, "OPERATORS", "OPERATORNAME");
		formEditor.ShowDialog();
		formEditor.Dispose();
		Zupolnit();
	}

	private void INN_Click(object sender, EventArgs e)
	{
		FormEditor formEditor = new FormEditor("ІНН оператора", InnOpT.Text, "OPERATORS", "INN");
		formEditor.ShowDialog();
		formEditor.Dispose();
		Zupolnit();
	}

	private void Pas_Click(object sender, EventArgs e)
	{
		FormEditor formEditor = new FormEditor("Пароль ключа ЕЦП", "*********", "OPERATORS", "KEYPASS");
		formEditor.ShowDialog();
		formEditor.Dispose();
		Zupolnit();
	}

	private void Adr_Click(object sender, EventArgs e)
	{
		FormEditor formEditor = new FormEditor("Адреса торгової точки", AtorgT.Text, "TAXOBJECTS", "POINTADDR");
		formEditor.ShowDialog();
		formEditor.Dispose();
		Zupolnit();
	}

	private void NamT_Click(object sender, EventArgs e)
	{
		FormEditor formEditor = new FormEditor("Назва торгової точки", NtorgT.Text, "TAXOBJECTS", "POINTNAME");
		formEditor.ShowDialog();
		formEditor.Dispose();
		Zupolnit();
	}

	private void NamO_Click(object sender, EventArgs e)
	{
		FormEditor formEditor = new FormEditor("Назва організації", NorgT.Text, "TAXOBJECTS", "ORGNAME");
		formEditor.ShowDialog();
		formEditor.Dispose();
		Zupolnit();
	}

	private void IPN_Click(object sender, EventArgs e)
	{
		FormEditor formEditor = new FormEditor("ІПН платника ПДВ", InnT.Text, "TAXOBJECTS", "INN");
		formEditor.ShowDialog();
		formEditor.Dispose();
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
		if (!All.A.Status)
		{
			Interaction.MsgBox("Увага! Необхідно підключення!", MsgBoxStyle.Exclamation, "Перевірка налаштувань");
			return;
		}
		All.SF.SignatureStart();
		int acskSettings = All.A.AcskSettings;
		All.A.AcskSettings = All.A.AcskSettingsTemp;
		All.SF.SetServer();
		FormTest formTest = new FormTest(InnOpT.Text.Trim());
		formTest.ShowDialog();
		formTest.Dispose();
		All.A.AcskSettings = acskSettings;
		All.SF.SetServer();
	}

	private void FormNewPro_Closing(object sender, CancelEventArgs e)
	{
		if (Operators.CompareString(All.A.FiscalMode, All.URLfact, TextCompare: false) == 0 && !NewBase && All.A.FN.Length > 9 && !File.Exists(Strings.Replace(All.A.FileN, All.A.FN, All.A.FN + "_TS")))
		{
			string fnS = All.A.FN + "_TS";
			CreateTables(fnS);
			CreateRow(fnS, ParOld);
		}
	}

	private void ImportDat_Click(object sender, EventArgs e)
	{
		All.SF.SignatureStart();
		if ((Operators.CompareString(KeyOpT.Text.Trim(), "", TextCompare: false) == 0) | (Operators.CompareString(PasOpT.Text.Trim(), "", TextCompare: false) == 0))
		{
			Interaction.MsgBox("Обов'язкові поля для завантаження даних з кабінету податкової:\r\n- Ключ ЕЦП\r\n- Пароль до ключа ЕЦП\r\n- АЦСК", MsgBoxStyle.Exclamation, "Завантаження даних");
			return;
		}
		string text = All.MyDoc() + "\\WebCheck\\Temp\\objects.txt";
		if (File.Exists(text))
		{
			Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(text);
		}
		if (File.Exists(text + ".p7s"))
		{
			Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(text + ".p7s");
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
		if (Operators.CompareString(text2.Trim(), "", TextCompare: false) != 0)
		{
			All.LgAll.SaveTextToLogAll("NEW PRRO JSON", text2);
			FormImport formImport = new FormImport(text2);
			formImport.ShowDialog();
			formImport.Dispose();
			if (All.InfaImport.NumFiscal.Length == 10)
			{
				FnT.Text = All.InfaImport.NumFiscal;
				TinT.Text = All.InfaImport.TIN;
				InnT.Text = All.InfaImport.IPN;
				NorgT.Text = All.InfaImport.OrgName;
				NtorgT.Text = All.InfaImport.Name;
				AtorgT.Text = All.InfaImport.Address;
			}
			FioOpT.Focus();
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
			if (Operators.CompareString(@string.Trim(), "", TextCompare: false) == 0)
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
		TypErrStrCert typErrStrCert = All.SF.Cert(KeyOpT.Text, PasOpT.Text);
		if (typErrStrCert.errCode > 0)
		{
			Interaction.MsgBox("Помилка завантаження даних з сервера податкової:\r\n" + TextError, MsgBoxStyle.Exclamation, "Помилка завантаження даних!");
			return;
		}
		Interaction.MsgBox(TextError + " \r\nІнформація про сертифікат ключа: \r\n- Власник: " + typErrStrCert.ReturnSUBJCN + " \r\n- Дата початку дії: " + typErrStrCert.ReturnStart + " \r\n- Дата закінчення: " + typErrStrCert.ReturnEnd + " \r\n- TIN: " + typErrStrCert.ReturnTIN + " \r\n- Номер сертифікату: \r\n" + typErrStrCert.ReturnSerial + " \r\nне зареєстровано в податковій. \r\nПодайте форму 5-ПРРО в кабінети платника податків", MsgBoxStyle.Exclamation, "Помилка завантаження даних!");
	}

	private void CheckBoxManual_CheckedChanged(object sender, EventArgs e)
	{
		if (CheckBoxManual.Checked)
		{
			TinT.ReadOnly = false;
			FnT.ReadOnly = false;
			return;
		}
		TinT.Text = "";
		TinT.ReadOnly = true;
		FnT.Text = "";
		FnT.ReadOnly = true;
	}
}
